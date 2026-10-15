//! Internal USB driver
//!
//! The goal is to keep this somewhat agnostic from the usb-device
//! bus behaviors, so that it could be used separately. However, it's
//! not yet exposed in the package's API.

use crate::{buffer, gpt, ral};
use usb_device::{
    UsbDirection, UsbError,
    bus::PollResult,
    endpoint::{EndpointAddress, EndpointType},
};

/// Direct index to the OUT control endpoint
fn ctrl_ep0_out() -> EndpointAddress {
    // Constructor not currently const. Otherwise, this would
    // be a const.
    EndpointAddress::from_parts(0, UsbDirection::Out)
}

/// Direct index to the IN control endpoint
fn ctrl_ep0_in() -> EndpointAddress {
    EndpointAddress::from_parts(0, UsbDirection::In)
}

/// USB low / full / high speed setting.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Speed {
    /// Throttle to low / full speeds.
    ///
    /// If a host is capable of high-speed, this will prevent
    /// the device from enumerating as a high-speed device.
    LowFull,
    /// High speed.
    ///
    /// A high-speed device can still interface a low / full
    /// speed host, so use this setting for the most flexibility.
    #[default]
    High,
}

/// A USB driver
///
/// After you allocate a `Driver` with [`new()`](Driver::new), you must
///
/// - call [`initialize()`](Driver::initialize) once
/// - supply endpoint memory with [`set_endpoint_memory()`](USB::set_endpoint_memory)
pub struct Driver {
    usb: ral::AnyUsbInstance,
    phy: ral::AnyUsbphyInstance,
    buffer_allocator: buffer::Allocator,
    ep_allocator: crate::state::EndpointAllocator<'static>,
    /// Track which read endpoints have completed, so as to not
    /// confuse the device and appear out of sync with poll() calls.
    ///
    /// Persisting the ep_out mask across poll() calls lets us make
    /// sure that results of ep_read calls match what's signaled from
    /// poll() calls. During testing, we saw that poll() wouldn't signal
    /// ep_out complete. But, the class could still call ep_read(), and
    /// it would return data. The usb-device test_class treats that as
    /// a failure, so we should keep behaviors consistent.
    ep_out: u16,
}

impl Driver {
    /// Create a new `Driver`
    ///
    /// Creation does nothing except for assign static memory to the driver.
    /// After creating the driver, call [`initialize()`](USB::initialize).
    ///
    /// # Panics
    ///
    /// Panics if the endpoint bufer or state has already been assigned to another USB
    /// driver.
    pub fn new<const N: u8, const SIZE: usize, const EP_COUNT: usize>(
        instances: crate::Instances<N>,
        buffer: &'static crate::buffer::EndpointMemory<SIZE>,
        state: &'static crate::state::EndpointState<EP_COUNT>,
    ) -> Self {
        let ral::ErasedInstances { usb, usbphy: phy } = ral::erase_instances(instances);
        let ep_allocator = state.allocator().expect("Endpoint state already assigned");
        Driver {
            usb,
            phy,
            buffer_allocator: buffer
                .allocator()
                .expect("Endpoint memory already assigned"),
            ep_allocator,
            ep_out: 0,
        }
    }

    /// Initialize the USB physical layer, and the USB core registers
    ///
    /// Assumes that the CCM clock gates are enabled, and the PLL is on.
    ///
    /// You **must** call this once, before creating the complete USB
    /// bus.
    pub fn initialize(&mut self, speed: Speed) {
        ral::write_reg!(ral::usbphy, self.phy, CTRL_SET, SFTRST: 1);
        ral::write_reg!(ral::usbphy, self.phy, CTRL_CLR, SFTRST: 1);
        ral::write_reg!(ral::usbphy, self.phy, CTRL_CLR, CLKGATE: 1);
        ral::write_reg!(ral::usbphy, self.phy, PWD, 0);

        ral::write_reg!(ral::usb, self.usb, USBCMD, RST: 1);
        while ral::read_reg!(ral::usb, self.usb, USBCMD, RST == 1) {}
        // ITC is reset to some non-immediate value. Use the 'immediate' value by default.
        // (Note: this also zeros all other USBCMD fields.)
        ral::write_reg!(ral::usb, self.usb, USBCMD, ITC: 0);

        ral::write_reg!(ral::usb, self.usb, USBMODE, CM: CM_2, SLOM: 1);
        ral::modify_reg!(ral::usb, self.usb, PORTSC1, PFSC: (speed == Speed::LowFull) as u32);

        ral::modify_reg!(ral::usb, self.usb, USBSTS, |usbsts| usbsts);
        // Disable interrupts by default
        ral::write_reg!(ral::usb, self.usb, USBINTR, 0);

        ral::write_reg!(
            ral::usb,
            self.usb,
            ASYNCLISTADDR,
            self.ep_allocator.qh_list_addr() as u32
        )
    }

    /// Enable zero-length termination (ZLT) for the given endpoint
    ///
    /// When ZLT is enabled, software does not need to send a zero-length packet
    /// to terminate a transfer where the number of bytes equals the max packet size.
    /// The hardware will send this zero-length packet itself. By default, ZLT is off,
    /// and software is expected to send these packets. Enable this if you're confident
    /// that your (third-party) device / USB class isn't already sending these packets.
    ///
    /// This call does nothing if the endpoint isn't allocated.
    pub fn enable_zlt(&mut self, ep_addr: EndpointAddress) {
        if let Some(ep) = self.ep_allocator.endpoint_mut(ep_addr) {
            ep.enable_zlt();
        }
    }

    /// Enable (`true`) or disable (`false`) USB interrupts
    pub fn set_interrupts(&mut self, interrupts: bool) {
        if interrupts {
            // Keep this in sync with the poll() behaviors
            ral::modify_reg!(ral::usb, self.usb, USBINTR, UE: 1, URE: 1);
        } else {
            ral::modify_reg!(ral::usb, self.usb, USBINTR, UE: 0, URE: 0);
        }
    }

    /// Acquire mutable access to a GPT timer
    pub fn gpt_mut<R>(&mut self, instance: gpt::Instance, f: impl FnOnce(&mut gpt::Gpt) -> R) -> R {
        let mut gpt = gpt::Gpt::new(&mut self.usb, instance);
        f(&mut gpt)
    }

    pub fn set_address(&mut self, address: u8) {
        // See the "quirk" note in the UsbBus impl. We're using USBADRA to let
        // the hardware set the address before the status phase.
        ral::write_reg!(ral::usb, self.usb, DEVICEADDR, USBADR: address as u32, USBADRA: 1);
        debug!("ADDRESS {=u8}", address);
    }

    pub fn attach(&mut self) {
        ral::modify_reg!(ral::usb, self.usb, USBCMD, RS: 1);
    }

    /// Clear the Run/Stop bit, halting the controller and removing the D+
    /// pull-up. The host sees the device disconnect.
    ///
    /// This is the symmetric counterpart to [`attach()`](Driver::attach): pair
    /// `detach()` with a brief hold and a follow-up `attach()` to force the host
    /// to re-enumerate the device (see [`BusAdapter::force_reset`]).
    ///
    /// [`BusAdapter::force_reset`]: crate::BusAdapter
    pub fn detach(&mut self) {
        ral::modify_reg!(ral::usb, self.usb, USBCMD, RS: 0);
    }

    pub fn bus_reset(&mut self) {
        ral::modify_reg!(ral::usb, self.usb, ENDPTSTAT, |endptstat| endptstat);

        ral::modify_reg!(ral::usb, self.usb, ENDPTCOMPLETE, |endptcomplete| {
            endptcomplete
        });
        ral::modify_reg!(ral::usb, self.usb, ENDPTNAK, |endptnak| endptnak);
        ral::write_reg!(ral::usb, self.usb, ENDPTNAKEN, 0);

        while ral::read_reg!(ral::usb, self.usb, ENDPTPRIME) != 0 {}
        ral::write_reg!(ral::usb, self.usb, ENDPTFLUSH, u32::MAX);
        while ral::read_reg!(ral::usb, self.usb, ENDPTFLUSH) != 0 {}

        debug_assert!(
            ral::read_reg!(ral::usb, self.usb, PORTSC1, PR == 1),
            "Took too long to handle bus reset"
        );
        debug!("RESET");

        // Under the transfer feature, clear the software-side queue state on
        // every non-control endpoint so stale records don't survive across resets.
        #[cfg(feature = "transfer")]
        for ep in self.ep_allocator.nonzero_endpoints_iter_mut() {
            ep.clear_transfers(&self.usb);
        }

        self.initialize_endpoints();
    }

    /// Check if the endpoint is valid
    pub fn is_allocated(&self, addr: EndpointAddress) -> bool {
        self.ep_allocator.endpoint(addr).is_some()
    }

    /// Read either a setup, or a data buffer, from EP0 OUT
    ///
    /// # Panics
    ///
    /// Panics if EP0 OUT isn't allocated.
    pub fn ctrl0_read(&mut self, buffer: &mut [u8]) -> Result<usize, UsbError> {
        let ctrl_out = self.ep_allocator.endpoint_mut(ctrl_ep0_out()).unwrap();
        if ctrl_out.has_setup(&self.usb) && buffer.len() >= 8 {
            debug!("EP0 Out SETUP");
            let setup = ctrl_out.read_setup(&self.usb);
            buffer[..8].copy_from_slice(&setup.to_le_bytes());

            if !ctrl_out.is_primed(&self.usb) {
                ctrl_out.clear_nack(&self.usb);
                let max_packet_len = ctrl_out.max_packet_len();
                ctrl_out.schedule_transfer(&self.usb, max_packet_len);
            }

            Ok(8)
        } else {
            ctrl_out.check_errors()?;

            if ctrl_out.is_primed(&self.usb) {
                return Err(UsbError::WouldBlock);
            }

            ctrl_out.clear_complete(&self.usb);
            ctrl_out.clear_nack(&self.usb);

            let read = ctrl_out.read(buffer);
            debug!("EP0 Out {=usize}", read);
            let max_packet_len = ctrl_out.max_packet_len();
            ctrl_out.schedule_transfer(&self.usb, max_packet_len);

            Ok(read)
        }
    }

    /// Write to the host from EP0 IN
    ///
    /// Schedules the next OUT transfer to satisfy a status phase.
    ///
    /// # Panics
    ///
    /// Panics if EP0 IN isn't allocated, or if EP0 OUT isn't allocated.
    pub fn ctrl0_write(&mut self, buffer: &[u8]) -> Result<usize, UsbError> {
        let ctrl_in = self.ep_allocator.endpoint_mut(ctrl_ep0_in()).unwrap();
        debug!("EP0 In {=usize}", buffer.len());
        ctrl_in.check_errors()?;

        if ctrl_in.is_primed(&self.usb) {
            return Err(UsbError::WouldBlock);
        }

        ctrl_in.clear_nack(&self.usb);

        let written = ctrl_in.write(buffer);
        ctrl_in.schedule_transfer(&self.usb, written);

        // Might need an OUT schedule for a status phase...
        let ctrl_out = self.ep_allocator.endpoint_mut(ctrl_ep0_out()).unwrap();
        if !ctrl_out.is_primed(&self.usb) {
            ctrl_out.clear_complete(&self.usb);
            ctrl_out.clear_nack(&self.usb);
            ctrl_out.schedule_transfer(&self.usb, 0);
        }

        Ok(written)
    }

    /// Read data from an endpoint, and schedule the next transfer
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated.
    pub fn ep_read(&mut self, buffer: &mut [u8], addr: EndpointAddress) -> Result<usize, UsbError> {
        #[cfg(feature = "transfer")]
        {
            self.ep_read_queued(buffer, addr)
        }
        #[cfg(not(feature = "transfer"))]
        {
            let ep = self.ep_allocator.endpoint_mut(addr).unwrap();
            debug!("EP{=usize} Out", ep.address().index());
            ep.check_errors()?;

            if ep.is_primed(&self.usb) || (self.ep_out & (1 << ep.address().index()) == 0) {
                return Err(UsbError::WouldBlock);
            }

            ep.clear_complete(&self.usb); // Clears self.ep_out bit on the next poll() call...
            ep.clear_nack(&self.usb);

            let read = ep.read(buffer);

            let max_packet_len = ep.max_packet_len();
            ep.schedule_transfer(&self.usb, max_packet_len);

            Ok(read)
        }
    }

    /// Write data to an endpoint
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated.
    pub fn ep_write(&mut self, buffer: &[u8], addr: EndpointAddress) -> Result<usize, UsbError> {
        #[cfg(feature = "transfer")]
        {
            self.ep_write_queued(buffer, addr)
        }
        #[cfg(not(feature = "transfer"))]
        {
            let ep = self.ep_allocator.endpoint_mut(addr).unwrap();
            ep.check_errors()?;

            if ep.is_primed(&self.usb) {
                return Err(UsbError::WouldBlock);
            }

            ep.clear_nack(&self.usb);

            let written = ep.write(buffer);
            ep.schedule_transfer(&self.usb, written);

            Ok(written)
        }
    }

    /// Stall an endpoint
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated
    pub fn ep_stall(&mut self, stall: bool, addr: EndpointAddress) {
        let ep = self.ep_allocator.endpoint_mut(addr).unwrap();
        ep.set_stalled(&self.usb, stall);

        // Re-prime any OUT endpoints if we're unstalling. Only prime an *enabled*
        // endpoint: priming a disabled OUT endpoint (e.g. a class clearing stalls
        // in its reset() before SET_CONFIGURATION enables endpoints) leaves a stale
        // transfer descriptor that the controller never completes once the endpoint
        // is later enabled.
        if !stall && addr.direction() == UsbDirection::Out && ep.is_enabled(&self.usb) {
            #[cfg(feature = "transfer")]
            {
                // Under the transfer feature: eager (depth>1) endpoints re-prime
                // their staging transfers on unstall. Depth-1 (lazy) endpoints stay
                // lazy — they never have a standing prime outside of an ep_read call.
                if ep.packet_depth() > 1 && !ep.is_primed(&self.usb) {
                    let depth = ep.packet_depth();
                    let mps = ep.max_packet_len();
                    for slot in 0..depth {
                        if ep.buffers[slot].is_some() {
                            let ptr = ep.staging_buf_ptr(slot);
                            // submit_inner returns WouldBlock if TD budget is full;
                            // ignore in this recovery path — we prime what fits.
                            let _ = ep.submit_inner(&self.usb, ptr, mps, Some(slot as u8));
                        }
                    }
                }
            }
            #[cfg(not(feature = "transfer"))]
            {
                if !ep.is_primed(&self.usb) {
                    let max_packet_len = ep.max_packet_len();
                    ep.schedule_transfer(&self.usb, max_packet_len);
                }
            }
        }
    }

    // -------------------------------------------------------------------------
    // Transfer-queue packet path (feature = "transfer")
    // -------------------------------------------------------------------------

    /// Queue-backed `ep_read` (OUT, non-control endpoints).
    ///
    /// Lazy/eager rule:
    /// - `poll_transfer` inner: ack ENDPTCOMPLETE before inspecting TD status.
    /// - `Some(Ok(n))` with a `staging_slot`: copy bytes from the staging buffer,
    ///   re-prime if `packet_depth > 1`.
    /// - `None` and queue empty: lazy prime (depth-1) or already primed (depth>1).
    /// - `None` with pending transfers: WouldBlock, no new prime.
    #[cfg(feature = "transfer")]
    fn ep_read_queued(
        &mut self,
        buffer: &mut [u8],
        addr: EndpointAddress,
    ) -> Result<usize, UsbError> {
        debug_assert!(
            addr.index() != 0,
            "ep_read_queued must not be called on EP0"
        );
        if addr.index() == 0 {
            return Err(UsbError::InvalidEndpoint);
        }

        let ep = self.ep_allocator.endpoint_mut(addr).unwrap();
        debug!("EP{=usize} Out (queued)", ep.address().index());
        ep.check_errors()?;

        // clear_nack at entry (master does it before polling).
        ep.clear_nack(&self.usb);

        // Ack ENDPTCOMPLETE before inspecting TD status.
        ep.clear_complete(&self.usb);

        match ep.poll_transfer_with_slot() {
            Some((Ok(n), Some(slot))) => {
                // Packet-path (staging) transfer retired. Copy bytes out.
                let n_out = n.min(buffer.len());
                ep.staging(slot as usize)
                    .volatile_read(&mut buffer[..n_out]);

                // Eager: re-prime this slot's staging buffer immediately.
                let depth = ep.packet_depth();
                if depth > 1 {
                    let mps = ep.max_packet_len();
                    let ptr = ep.staging_buf_ptr(slot as usize);
                    let _ = ep.submit_inner(&self.usb, ptr, mps, Some(slot));
                }

                Ok(n_out)
            }
            Some((Ok(_), None)) => {
                // Zero-copy transfer at the head while class calls ep_read — the
                // class mixed packet reads with a zero-copy data phase.
                debug_assert!(
                    false,
                    "ep_read retired a zero-copy transfer; class mixed packet and zero-copy I/O"
                );
                Err(UsbError::InvalidState)
            }
            Some((Err(e), _)) => Err(e),
            None => {
                // Queue empty (or oldest still in flight).
                if ep.pending_transfers() == 0 {
                    // Lazy prime: submit one 1-packet staging transfer.
                    let mps = ep.max_packet_len();
                    let ptr = ep.staging_buf_ptr(0);
                    ep.submit_inner(&self.usb, ptr, mps, Some(0))?;
                }
                // Either we just primed or there's already something in flight.
                Err(UsbError::WouldBlock)
            }
        }
    }

    /// Queue-backed `ep_write` (IN, non-control endpoints).
    ///
    /// Copies `buffer` into the staging buffer for the next free slot and
    /// submits a 1-packet staging transfer. Returns `WouldBlock` when the TD
    /// budget is full (depth-1 = second write before first retires).
    #[cfg(feature = "transfer")]
    fn ep_write_queued(&mut self, buffer: &[u8], addr: EndpointAddress) -> Result<usize, UsbError> {
        debug_assert!(
            addr.index() != 0,
            "ep_write_queued must not be called on EP0"
        );
        if addr.index() == 0 {
            return Err(UsbError::InvalidEndpoint);
        }

        let ep = self.ep_allocator.endpoint_mut(addr).unwrap();
        ep.check_errors()?;

        // Reap retired fire-and-forget staging records so they don't pin the
        // FIFO head / TD budget (the class never polls its packet writes).
        ep.reap_staging_in();

        // Depth-1 staging rule: while a staging packet is still queued, the
        // controller owns the slot-0 staging buffer — a new write would
        // overwrite the in-flight packet (master parity: `is_primed` →
        // WouldBlock). Zero-copy records don't block the packet path beyond
        // the TD budget.
        if ep.has_pending_staging() || ep.tds_free() == 0 {
            return Err(UsbError::WouldBlock);
        }

        ep.clear_nack(&self.usb);

        // Pick the next free staging slot (slot 0 always exists; multi-depth
        // uses slots round-robin via q_len, but for IN we just use slot 0 always
        // since IN is not primed ahead).
        let slot = 0usize;
        let written = {
            let buf = ep.staging_mut(slot);
            let written = buf.volatile_write(&buffer[..buffer.len().min(buf.len())]);
            let mps = buf.len();
            buf.clean_invalidate_dcache(mps);
            written
        };
        let ptr = ep.staging_buf_ptr(slot);
        ep.submit_inner(&self.usb, ptr, written, Some(slot as u8))?;

        Ok(written)
    }

    // -------------------------------------------------------------------------
    // Transfer-queue public API (feature = "transfer")
    // -------------------------------------------------------------------------

    /// Queue a zero-copy IN or OUT transfer on a non-control endpoint.
    ///
    /// Zero-copy: the memory at `ptr[..len]` must remain valid and untouched
    /// until `ep_poll_transfer` retires it.
    ///
    /// # Panics (debug)
    ///
    /// Panics in debug builds if called on EP0.
    #[cfg(feature = "transfer")]
    pub fn ep_submit(
        &mut self,
        addr: EndpointAddress,
        ptr: *mut u8,
        len: usize,
    ) -> Result<(), UsbError> {
        debug_assert!(addr.index() != 0, "ep_submit must not be called on EP0");
        if addr.index() == 0 {
            return Err(UsbError::InvalidEndpoint);
        }
        let ep = self
            .ep_allocator
            .endpoint_mut(addr)
            .ok_or(UsbError::InvalidEndpoint)?;
        ep.submit_transfer(&self.usb, ptr, len)
    }

    /// Retire the oldest completed transfer on `addr`, if any.
    ///
    /// # Panics (debug)
    ///
    /// Panics in debug builds if called on EP0.
    #[cfg(feature = "transfer")]
    pub fn ep_poll_transfer(&mut self, addr: EndpointAddress) -> Option<Result<usize, UsbError>> {
        debug_assert!(
            addr.index() != 0,
            "ep_poll_transfer must not be called on EP0"
        );
        if addr.index() == 0 {
            return Some(Err(UsbError::InvalidEndpoint));
        }
        // Ack ENDPTCOMPLETE before inspecting TD status (per spec: substep 3b).
        if let Some(ep) = self.ep_allocator.endpoint_mut(addr) {
            ep.clear_complete(&self.usb);
            ep.poll_transfer()
        } else {
            Some(Err(UsbError::InvalidEndpoint))
        }
    }

    /// Number of queued (not yet retired) transfers on `addr`.
    #[cfg(feature = "transfer")]
    pub fn ep_pending_transfers(&self, addr: EndpointAddress) -> usize {
        if addr.index() == 0 {
            return 0;
        }
        self.ep_allocator
            .endpoint(addr)
            .map(|ep| ep.pending_transfers())
            .unwrap_or(0)
    }

    /// Cancel every queued transfer on `addr`: flush the hardware prime and
    /// drop all software queue records (staging and zero-copy alike).
    ///
    /// For class-level reconfiguration of a *shared* endpoint — e.g. a
    /// SET_INTERFACE alternate-setting switch where the previous alt left a
    /// primed transfer that would otherwise swallow the new alt's data
    /// (HW-observed 2026-06-11: a stale 512-byte BOT CBW staging prime on the
    /// shared bulk-OUT consumed the first packet of the first UAS WRITE data
    /// phase). Bus reset and `enable_endpoints` already clear queues; this is
    /// the explicit per-endpoint hook for alt switches, which the bus layer
    /// never sees. No-op on EP0 and unallocated endpoints.
    #[cfg(feature = "transfer")]
    pub fn ep_cancel_transfers(&mut self, addr: EndpointAddress) {
        if addr.index() == 0 {
            return;
        }
        if let Some(ep) = self.ep_allocator.endpoint_mut(addr) {
            ep.clear_transfers(&self.usb);
        }
    }

    /// Allocate `depth - 1` extra max-packet staging buffers for `addr` and keep
    /// `depth` 1-packet OUT transfers primed through the packet path (eager
    /// read-ahead). Call after `UsbDevice` configuration, before traffic.
    ///
    /// `depth` must be ≥ 1. Only valid for OUT non-control endpoints; returns
    /// `Err(UsbError::InvalidEndpoint)` for IN or EP0.
    ///
    /// If the endpoint memory pool is exhausted mid-fill, returns
    /// `Err(UsbError::EndpointMemoryOverflow)`; already-allocated slots remain
    /// allocated but unused beyond the previous depth.
    #[cfg(feature = "transfer")]
    pub fn set_packet_queue_depth(
        &mut self,
        addr: EndpointAddress,
        depth: usize,
    ) -> Result<(), UsbError> {
        debug_assert!(
            addr.index() != 0,
            "set_packet_queue_depth must not be called on EP0"
        );
        if addr.index() == 0 || addr.direction() == UsbDirection::In {
            return Err(UsbError::InvalidEndpoint);
        }

        let mps = self
            .ep_allocator
            .endpoint_mut(addr)
            .ok_or(UsbError::InvalidEndpoint)?
            .max_packet_len();

        // Fill slots 1..depth with staging buffers. Each iteration re-borrows
        // the endpoint because buffer_allocator and ep_allocator are both on
        // self and Rust can't split borrow across the match.
        for slot in 1..depth {
            // Check if slot already filled before calling allocator.
            let needs_buf = self
                .ep_allocator
                .endpoint_mut(addr)
                .map(|ep| ep.buffers[slot].is_none())
                .unwrap_or(false);
            if needs_buf {
                match self.buffer_allocator.allocate(mps) {
                    Some(buf) => {
                        // Safety: ep was checked above; addr is non-control OUT, already validated.
                        if let Some(ep) = self.ep_allocator.endpoint_mut(addr) {
                            ep.buffers[slot] = Some(buf);
                        }
                    }
                    None => {
                        return Err(UsbError::EndpointMemoryOverflow);
                    }
                }
            }
        }

        // Set the configured depth.
        if let Some(ep) = self.ep_allocator.endpoint_mut(addr) {
            ep.set_packet_depth(depth);
        }

        // Submit `depth` 1-packet OUT staging transfers immediately.
        for slot in 0..depth {
            if let Some(ep) = self.ep_allocator.endpoint_mut(addr) {
                let mps = ep.max_packet_len();
                let ptr = ep.staging_buf_ptr(slot);
                if ep.tds_free() > 0 {
                    let _ = ep.submit_inner(&self.usb, ptr, mps, Some(slot as u8));
                }
            }
        }

        Ok(())
    }

    /// Checks if an endpoint is stalled
    ///
    /// Returns `false` for an endpoint that isn't allocated.
    pub fn is_ep_stalled(&self, addr: EndpointAddress) -> bool {
        self.ep_allocator
            .endpoint(addr)
            .is_some_and(|ep| ep.is_stalled(&self.usb))
    }

    /// Allocate a buffer from the endpoint memory
    pub fn allocate_buffer(&mut self, max_packet_len: usize) -> Option<buffer::Buffer> {
        self.buffer_allocator.allocate(max_packet_len)
    }

    /// Allocate a specific endpoint
    ///
    /// # Panics
    ///
    /// Panics if the endpoint is already allocated.
    pub fn allocate_ep(
        &mut self,
        addr: EndpointAddress,
        buffer: buffer::Buffer,
        kind: EndpointType,
    ) {
        self.ep_allocator
            .allocate_endpoint(addr, buffer, kind)
            .unwrap();

        debug!(
            "ALLOC EP{=usize} {} {}",
            addr.index(),
            addr.direction(),
            kind
        );
    }

    /// Invoked when the device transitions into the configured state
    pub fn on_configured(&mut self) {
        self.enable_endpoints();
        self.prime_endpoints();
    }

    /// Enable all non-zero endpoints
    ///
    /// This should only be called when the device is configured
    fn enable_endpoints(&mut self) {
        for ep in self.ep_allocator.nonzero_endpoints_iter_mut() {
            // Under the transfer feature, flush stale queue state before enabling.
            #[cfg(feature = "transfer")]
            ep.clear_transfers(&self.usb);
            ep.enable(&self.usb);
        }
    }

    /// Prime all non-zero, enabled OUT endpoints
    ///
    /// Under the transfer feature, eager (depth>1) OUT endpoints prime via the
    /// queue path; depth-1 endpoints stay lazy and are primed on first `ep_read`.
    fn prime_endpoints(&mut self) {
        for ep in self.ep_allocator.nonzero_endpoints_iter_mut() {
            if ep.is_enabled(&self.usb) && ep.address().direction() == UsbDirection::Out {
                #[cfg(feature = "transfer")]
                {
                    // Only prime eager endpoints here; lazy ones prime on demand.
                    if ep.packet_depth() > 1 {
                        let depth = ep.packet_depth();
                        let mps = ep.max_packet_len();
                        for slot in 0..depth {
                            if ep.buffers[slot].is_some() && ep.tds_free() > 0 {
                                let ptr = ep.staging_buf_ptr(slot);
                                let _ = ep.submit_inner(&self.usb, ptr, mps, Some(slot as u8));
                            }
                        }
                    }
                }
                #[cfg(not(feature = "transfer"))]
                {
                    let max_packet_len = ep.max_packet_len();
                    ep.schedule_transfer(&self.usb, max_packet_len);
                }
            }
        }
    }

    /// Initialize (or reinitialize) all non-zero endpoints
    fn initialize_endpoints(&mut self) {
        for ep in self.ep_allocator.nonzero_endpoints_iter_mut() {
            ep.initialize(&self.usb);
        }
    }

    /// Poll for reset or USB traffic
    pub fn poll(&mut self) -> PollResult {
        let usbsts = ral::read_reg!(ral::usb, self.usb, USBSTS);
        use ral::usb::USBSTS;

        if usbsts & USBSTS::URI::mask != 0 {
            ral::write_reg!(ral::usb, self.usb, USBSTS, URI: 1);
            return PollResult::Reset;
        }

        if usbsts & USBSTS::UI::mask != 0 {
            ral::write_reg!(ral::usb, self.usb, USBSTS, UI: 1);

            trace!(
                "ENDPTSETUPSTAT: {=u32:#010X}  ENDPTCOMPLETE: {=u32:#010X}",
                ral::read_reg!(ral::usb, self.usb, ENDPTSETUPSTAT),
                ral::read_reg!(ral::usb, self.usb, ENDPTCOMPLETE)
            );
            // Note: could be complete in one register read, but this is a little
            // easier to comprehend...
            self.ep_out = ral::read_reg!(ral::usb, self.usb, ENDPTCOMPLETE, ERCE) as u16;

            let ep_in_complete = ral::read_reg!(ral::usb, self.usb, ENDPTCOMPLETE, ETCE);
            ral::write_reg!(ral::usb, self.usb, ENDPTCOMPLETE, ETCE: ep_in_complete);

            let ep_setup = ral::read_reg!(ral::usb, self.usb, ENDPTSETUPSTAT) as u16;

            PollResult::Data {
                ep_out: self.ep_out,
                ep_in_complete: ep_in_complete as u16,
                ep_setup,
            }
        } else {
            PollResult::None
        }
    }
}

/// A `Driver` over zeroed, leaked register blocks, for host tests that never
/// touch real hardware.
///
/// Each call leaks its own register blocks, endpoint memory and endpoint
/// state, so tests running on parallel threads share nothing. `initialize`
/// is not called: it spins on `USBCMD.RST`, which a plain memory block never
/// clears.
#[cfg(test)]
pub(crate) fn fake_driver() -> Driver {
    extern crate std;
    use std::boxed::Box;

    fn leaked_zeroed<T>() -> *const T {
        // Safety: the register blocks are `repr(C)` structs of plain integer cells.
        Box::leak(Box::new(unsafe { core::mem::zeroed::<T>() }))
    }

    let instances = crate::Instances::<1> {
        // Safety: each pointer refers to a leaked, zeroed block that lives for
        // the rest of the process and is not aliased elsewhere.
        usb: unsafe { imxrt_ral::usb::Instance::new(leaked_zeroed()) },
        usbnc: unsafe { imxrt_ral::usbnc::Instance::new(leaked_zeroed()) },
        usbphy: unsafe { imxrt_ral::usbphy::Instance::new(leaked_zeroed()) },
    };
    let buffer = Box::leak(Box::new(buffer::EndpointMemory::<1024>::new()));
    let state = Box::leak(Box::new(crate::state::EndpointState::max_endpoints()));
    Driver::new(instances, buffer, state)
}

// grcov exclude start
#[expect(clippy::panic_in_result_fn, reason = "test module")]
#[cfg(test)]
mod tests {
    use super::{Driver, fake_driver};
    use usb_device::{
        UsbDirection,
        endpoint::{EndpointAddress, EndpointType},
    };

    type TestResult = core::result::Result<(), &'static str>;

    fn allocate(driver: &mut Driver, addr: EndpointAddress) -> TestResult {
        let buffer = driver
            .allocate_buffer(64)
            .ok_or("endpoint memory exhausted")?;
        driver.allocate_ep(addr, buffer, EndpointType::Bulk);
        Ok(())
    }

    #[test]
    fn is_ep_stalled_is_false_for_unallocated_endpoint() {
        // Given a driver with nothing allocated
        let driver = fake_driver();

        // When the host names any endpoint, in either direction, up to the
        // 4-bit index `GET_STATUS(Endpoint)` can carry
        for index in 0..16 {
            for direction in [UsbDirection::In, UsbDirection::Out] {
                let addr = EndpointAddress::from_parts(index, direction);

                // Then the driver answers instead of panicking
                assert!(
                    !driver.is_ep_stalled(addr),
                    "unallocated EP{index} {direction:?} must not report a stall"
                );
            }
        }
    }

    #[test]
    fn is_ep_stalled_follows_allocated_endpoint() -> TestResult {
        // Given one allocated endpoint
        let mut driver = fake_driver();
        let addr = EndpointAddress::from_parts(1, UsbDirection::In);
        allocate(&mut driver, addr)?;
        assert!(
            !driver.is_ep_stalled(addr),
            "a fresh endpoint is not stalled"
        );

        // When it is stalled, then unstalled
        driver.ep_stall(true, addr);
        let stalled = driver.is_ep_stalled(addr);
        driver.ep_stall(false, addr);
        let unstalled = driver.is_ep_stalled(addr);

        // Then the answer tracks the endpoint
        assert!(stalled, "a stalled endpoint reports a stall");
        assert!(!unstalled, "an unstalled endpoint reports none");

        // And its neighbours stay unaffected
        let other = EndpointAddress::from_parts(1, UsbDirection::Out);
        assert!(
            !driver.is_ep_stalled(other),
            "the unallocated OUT twin must not report a stall"
        );
        Ok(())
    }
}
// grcov exclude stop
