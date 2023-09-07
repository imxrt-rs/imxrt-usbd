//! Internal USB driver
//!
//! The goal is to keep this somewhat agnostic from the usb-device
//! bus behaviors, so that it could be used separately. However, it's
//! not yet exposed in the package's API.

use crate::{buffer, gpt, ral};
use usb_device::{
    bus::PollResult,
    endpoint::{EndpointAddress, EndpointType},
    UsbDirection, UsbError,
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
    usb: ral::usb::Instance,
    phy: ral::usbphy::Instance,
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
    pub fn new<P: crate::Peripherals, const SIZE: usize, const EP_COUNT: usize>(
        peripherals: P,
        buffer: &'static crate::buffer::EndpointMemory<SIZE>,
        state: &'static crate::state::EndpointState<EP_COUNT>,
    ) -> Self {
        // Safety: taking static memory. Assumes that the provided
        // USB instance is a singleton, which is the only safe way for it
        // to exist.
        let ral::Instances { usb, usbphy: phy } = ral::instances(peripherals);
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
        debug!("ADDRESS {}", address);
    }

    pub fn attach(&mut self) {
        ral::modify_reg!(ral::usb, self.usb, USBCMD, RS: 1);
    }

    pub fn bus_reset(&mut self) {
        ral::modify_reg!(ral::usb, self.usb, ENDPTSTAT, |endptstat| endptstat);

        ral::modify_reg!(ral::usb, self.usb, ENDPTCOMPLETE, |endptcomplete| {
            endptcomplete
        });
        ral::modify_reg!(ral::usb, self.usb, ENDPTNAK, |endptnak| endptnak);
        ral::write_reg!(ral::usb, self.usb, ENDPTNAKEN, 0);

        while ral::read_reg!(ral::usb, self.usb, ENDPTPRIME) != 0 {}
        ral::write_reg!(ral::usb, self.usb, ENDPTFLUSH, u32::max_value());
        while ral::read_reg!(ral::usb, self.usb, ENDPTFLUSH) != 0 {}

        debug_assert!(
            ral::read_reg!(ral::usb, self.usb, PORTSC1, PR == 1),
            "Took too long to handle bus reset"
        );
        debug!("RESET");

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
            debug!("EP0 Out {}", read);
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
        debug!("EP0 In {}", buffer.len());
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
        let ep = self.ep_allocator.endpoint_mut(addr).unwrap();
        debug!("EP{} Out", ep.address().index());
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

    /// Write data to an endpoint
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated.
    pub fn ep_write(&mut self, buffer: &[u8], addr: EndpointAddress) -> Result<usize, UsbError> {
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

    /// Stall an endpoint
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated
    pub fn ep_stall(&mut self, stall: bool, addr: EndpointAddress) {
        let ep = self.ep_allocator.endpoint_mut(addr).unwrap();
        ep.set_stalled(&self.usb, stall);

        // Re-prime any OUT endpoints if we're unstalling
        if !stall && addr.direction() == UsbDirection::Out && !ep.is_primed(&self.usb) {
            let max_packet_len = ep.max_packet_len();
            ep.schedule_transfer(&self.usb, max_packet_len);
        }
    }

    /// Checks if an endpoint is stalled
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated
    pub fn is_ep_stalled(&self, addr: EndpointAddress) -> bool {
        self.ep_allocator
            .endpoint(addr)
            .unwrap()
            .is_stalled(&self.usb)
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

        debug!("ALLOC EP{} {:?} {:?}", addr.index(), addr.direction(), kind);
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
            ep.enable(&self.usb);
        }
    }

    /// Prime all non-zero, enabled OUT endpoints
    fn prime_endpoints(&mut self) {
        for ep in self.ep_allocator.nonzero_endpoints_iter_mut() {
            if ep.is_enabled(&self.usb) && ep.address().direction() == UsbDirection::Out {
                let max_packet_len = ep.max_packet_len();
                ep.schedule_transfer(&self.usb, max_packet_len);
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
                "{:X} {:X}",
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
