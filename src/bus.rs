//! USB bus implementation
//!
//! The bus
//!
//! - initializes the USB driver
//! - adapts the USB driver to meet the `usb-device` `Sync` requirements
//! - dispatches reads and writes to the proper endpoints
//! - exposes the i.MX RT-specific API to the user (`configure`, `set_interrupts`)
//!
//! Most of the interesting behavior happens in the driver.

use super::driver::Driver;
use crate::gpt;
use core::cell::RefCell;
use cortex_m::interrupt::{self, Mutex};
use usb_device::{
    UsbDirection,
    bus::{PollResult, UsbBus},
    endpoint::{EndpointAddress, EndpointType},
};

pub use super::driver::Speed;

/// Busy-wait cycles the D+ pull-up is held low during [`force_reset`], between
/// dropping the bus (`detach`) and re-asserting it (`attach`).
///
/// This is a *hardware*-timing requirement, not a software-synchronisation
/// delay: the host detects a disconnect purely electrically (D+ held at SE0),
/// and there is no device-observable signal for "the host noticed". A bounded
/// busy-wait is therefore the correct primitive — there is nothing to poll on.
///
/// The value is sized generously for the i.MX RT10xx M7 cores: at 600 MHz
/// (RT1064) this is ~10 ms, and longer at lower core clocks (~12 ms at the
/// 500 MHz RT1011). USB hosts register a disconnect within microseconds, so a
/// few-millisecond hold has a wide margin. The library cannot know the core
/// clock, so this is a conservative floor; tune it on the bench if a specific
/// host needs a longer (or wants a shorter) disconnect window.
///
/// [`force_reset`]: UsbBus::force_reset
const DETACH_HOLD_CYCLES: u32 = 6_000_000;

/// A full- and high-speed `UsbBus` implementation
///
/// The `BusAdapter` adapts the USB peripheral instances, and exposes a `UsbBus` implementation.
///
/// # Requirements
///
/// The driver assumes that you've prepared all USB clocks (CCM clock gates, CCM analog PLLs).
///
/// Before polling for USB class traffic, you must call [`configure()`](BusAdapter::configure())
/// *after* your device has been configured. This can be accomplished by polling the USB
/// device and checking its state until it's been configured. Once configured, use `UsbDevice::bus()`
/// to access the i.MX RT `BusAdapter`, and call `configure()`. You should only do this once.
/// After that, you may poll for class traffic.
///
/// # Example
///
/// This example shows you how to create a `BusAdapter`, build a simple USB device, and
/// prepare the device for class traffic.
///
/// Note that this example does not demonstrate USB class allocation or polling. See
/// your USB class' documentation for details. This example also skips the clock initialization.
///
/// ```no_run
/// use imxrt_ral as ral;
/// use imxrt_usbd::{BusAdapter, Instances};
///
/// static EP_MEMORY: imxrt_usbd::EndpointMemory<1024> = imxrt_usbd::EndpointMemory::new();
/// static EP_STATE: imxrt_usbd::EndpointState = imxrt_usbd::EndpointState::max_endpoints();
///
/// // TODO initialize clocks...
///
/// let instances = Instances {
///     usb: unsafe { ral::usb::USB::instance() },
///     usbnc: unsafe { ral::usbnc::USBNC::instance() },
///     usbphy: unsafe { ral::usbphy::USBPHY::instance() },
/// };
/// let bus_adapter = BusAdapter::new(
///     instances,
///     &EP_MEMORY,
///     &EP_STATE,
/// );
///
/// // Create the USB device...
/// use usb_device::prelude::*;
/// let bus_allocator = usb_device::bus::UsbBusAllocator::new(bus_adapter);
/// let mut device = UsbDeviceBuilder::new(&bus_allocator, UsbVidPid(0x5824, 0x27dd))
///     .strings(&[StringDescriptors::default().product("imxrt-usbd")]).unwrap()
///     // Other builder methods...
///     .build();
///
/// // Poll until configured...
/// loop {
///     if device.poll(&mut []) {
///         let state = device.state();
///         if state == usb_device::device::UsbDeviceState::Configured {
///             break;
///         }
///     }
/// }
///
/// // Configure the bus
/// device.bus().configure();
///
/// // Ready for class traffic!
/// ```
///
/// # Design
///
/// This section talks about the driver design. It assumes that
/// you're familiar with the details of the i.MX RT USB peripheral. If you
/// just want to use the driver, you can skip this section.
///
/// ## Packets and transfers
///
/// All i.MX RT USB drivers manage queue heads (QH), and transfer
/// descriptors (TD). For the driver, each (QH) is assigned
/// only one (TD) to perform I/O. We then assume each TD describes a single
/// packet. This is simple to implement, but it means that the
/// driver can only have one packet in flight per endpoint. You're expected
/// to quickly respond to `poll()` outputs, and schedule the next transfer
/// in the time required for devices. This becomes more important as you
/// increase driver speeds.
///
/// The hardware can zero-length terminate (ZLT) packets as needed if you
/// call [`enable_zlt`](BusAdapter::enable_zlt). By default, this feature is
/// off, because most `usb-device` classes / devices take care to send zero-length
/// packets, and enabling this feature could interfere with the class / device
/// behaviors.
pub struct BusAdapter {
    usb: Mutex<RefCell<Driver>>,
    cs: Option<cortex_m::interrupt::CriticalSection>,
}

impl BusAdapter {
    /// Create a high-speed USB bus adapter
    ///
    /// This is equivalent to [`BusAdapter::with_speed`] when supplying [`Speed::High`]. See
    /// the `with_speed` documentation for more information.
    ///
    /// # Panics
    ///
    /// Panics if `buffer` or `state` has already been associated with another USB bus.
    pub fn new<const N: u8, const SIZE: usize, const EP_COUNT: usize>(
        instances: crate::Instances<N>,
        buffer: &'static crate::buffer::EndpointMemory<SIZE>,
        state: &'static crate::state::EndpointState<EP_COUNT>,
    ) -> Self {
        Self::with_speed(instances, buffer, state, Speed::High)
    }

    /// Create a USB bus adapter with the given speed
    ///
    /// Specify [`Speed::LowFull`] to throttle the USB data rate.
    ///
    /// When this function returns, the `BusAdapter` has initialized the PHY and USB core peripherals.
    /// The adapter takes ownership of these two peripherals for the lifetime of the driver.
    ///
    /// You must also provide a region of memory that will used for endpoint I/O. The
    /// memory region will be partitioned for the endpoints, based on their requirements.
    ///
    /// # Panics
    ///
    /// Panics if `buffer` or `state` has already been associated with another USB bus.
    pub fn with_speed<const N: u8, const SIZE: usize, const EP_COUNT: usize>(
        instances: crate::Instances<N>,
        buffer: &'static crate::buffer::EndpointMemory<SIZE>,
        state: &'static crate::state::EndpointState<EP_COUNT>,
        speed: Speed,
    ) -> Self {
        Self::init(instances, buffer, state, speed, None)
    }

    /// Create a USB bus adapter that never takes a critical section
    ///
    /// See [`BusAdapter::with_speed`] for general information.
    ///
    /// # Safety
    ///
    /// The returned object fakes its `Sync` safety. Specifically, the object
    /// will not take critical sections in its `&[mut] self` methods to ensure safe
    /// access. By using this object, you must manually hold the guarantees of
    /// `Sync` without the compiler's help.
    ///
    /// # Panics
    ///
    /// Panics if `buffer` or `state` has already been associated with another USB bus.
    pub unsafe fn without_critical_sections<
        const N: u8,
        const SIZE: usize,
        const EP_COUNT: usize,
    >(
        instances: crate::Instances<N>,
        buffer: &'static crate::buffer::EndpointMemory<SIZE>,
        state: &'static crate::state::EndpointState<EP_COUNT>,
        speed: Speed,
    ) -> Self {
        Self::init(
            instances,
            buffer,
            state,
            speed,
            // Safety: see the above API docs. Caller knows that we're faking our
            // Sync capability.
            Some(unsafe { cortex_m::interrupt::CriticalSection::new() }),
        )
    }

    fn init<const N: u8, const SIZE: usize, const EP_COUNT: usize>(
        instances: crate::Instances<N>,
        buffer: &'static crate::buffer::EndpointMemory<SIZE>,
        state: &'static crate::state::EndpointState<EP_COUNT>,
        speed: Speed,
        cs: Option<cortex_m::interrupt::CriticalSection>,
    ) -> Self {
        let mut usb = Driver::new(instances, buffer, state);

        usb.initialize(speed);

        BusAdapter {
            usb: Mutex::new(RefCell::new(usb)),
            cs,
        }
    }
    /// Enable (`true`) or disable (`false`) interrupts for this USB peripheral
    ///
    /// The interrupt causes are implementation specific. To handle the interrupt,
    /// call [`poll()`](BusAdapter::poll).
    pub fn set_interrupts(&self, interrupts: bool) {
        self.with_usb_mut(|usb| usb.set_interrupts(interrupts));
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
    pub fn enable_zlt(&self, ep_addr: EndpointAddress) {
        self.with_usb_mut(|usb| usb.enable_zlt(ep_addr));
    }

    /// Reset the data toggle of the given endpoint to DATA0
    ///
    /// Call this from firmware that observes the host's CLEAR_FEATURE(ENDPOINT_HALT)
    /// request: USB 2.0 §9.4.5 has the host reset its toggle for that endpoint, and
    /// the device must follow. No class should call it for a routine unstall.
    /// `usb-device` classes unstall endpoints in the normal course of work (for
    /// example `usbd-storage` before every command), and the host does not reset
    /// its toggle then, so [`UsbBus::set_stalled`] leaves the toggle alone.
    ///
    /// This call does nothing on endpoint 0 or if the endpoint isn't allocated.
    pub fn reset_data_toggle(&self, ep: EndpointAddress) {
        self.with_usb_mut(|usb| usb.ep_reset_data_toggle(ep));
    }

    /// Immutable access to the USB peripheral
    fn with_usb<R>(&self, func: impl FnOnce(&Driver) -> R) -> R {
        let with_cs = |cs: &'_ _| {
            let usb = self.usb.borrow(cs);
            let usb = usb.borrow();
            func(&usb)
        };
        if let Some(cs) = &self.cs {
            with_cs(cs)
        } else {
            interrupt::free(with_cs)
        }
    }

    /// Mutable access to the USB peripheral
    fn with_usb_mut<R>(&self, func: impl FnOnce(&mut Driver) -> R) -> R {
        let with_cs = |cs: &'_ _| {
            let usb = self.usb.borrow(cs);
            let mut usb = usb.borrow_mut();
            func(&mut usb)
        };
        if let Some(cs) = &self.cs {
            with_cs(cs)
        } else {
            interrupt::free(with_cs)
        }
    }

    /// Apply device configurations, and perform other post-configuration actions
    ///
    /// You must invoke this once, and only after your device has been configured. If
    /// the device is reset and reconfigured, you must invoke `configure()` again. See
    /// the top-level example for how this could be achieved.
    pub fn configure(&self) {
        self.with_usb_mut(|usb| {
            usb.on_configured();
            debug!("CONFIGURED");
        });
    }

    /// Acquire one of the GPT timer instances.
    ///
    /// `instance` identifies which GPT instance you're accessing.
    /// This may take a critical section for the duration of `func`.
    ///
    /// # Panics
    ///
    /// Panics if the GPT instance is already borrowed. This could happen
    /// if you call `gpt_mut` again within the `func` callback.
    pub fn gpt_mut<R>(&self, instance: gpt::Instance, func: impl FnOnce(&mut gpt::Gpt) -> R) -> R {
        self.with_usb_mut(|usb| usb.gpt_mut(instance, func))
    }
}

/// Transfer-queue extension API for [`BusAdapter`].
///
/// # Transfer-queue overview
///
/// This API provides a zero-copy, multi-transfer-in-flight path for bulk and
/// interrupt endpoints alongside the existing `UsbBus` single-packet path.
///
/// ## Retire law (EHCI)
///
/// A dTD is owned by the USB controller from the moment it is primed until the
/// ACTIVE bit clears. **No field of the dTD — and no byte of the associated DMA
/// buffer — may be read or written by software while ACTIVE is set.** Violating
/// this rule produces silent data corruption. `poll_transfer` / `poll_transfer`
/// enforce the retire law: they inspect ACTIVE before returning bytes.
///
/// ## Short-packet contract (OUT endpoints)
///
/// A short packet (fewer bytes than the TD announced) retires the current dTD
/// immediately and leaves any subsequent linked dTDs in the hardware queue. For
/// `submit_read`, callers should size OUT transfers to the announced data length
/// (e.g. a bulk CBW is exactly 31 bytes). Misbehaving hosts are recovered via
/// `bus_reset` / `UsbBus::reset`.
///
/// ## Lazy vs. eager OUT priming
///
/// Depth-1 OUT endpoints (the default) never have a staging transfer primed
/// unless the class explicitly calls `UsbBus::read`. This is the *lazy* rule:
/// it prevents the controller from swallowing a packet into a staging buffer
/// while a zero-copy data-phase transfer is active.
///
/// Endpoints configured with `set_packet_queue_depth(ep, N > 1)` are *eager*:
/// up to N staging transfers are kept primed at all times (CDC receive path).
///
/// ## Memory safety note
///
/// `submit_write` and `submit_read` accept raw slices and pass them to the DMA
/// engine. The controller writes/reads the memory asynchronously after the call
/// returns. Callers must guarantee that the slice remains valid and unmodified
/// until `poll_transfer` retires the record. Whether these methods should be
/// `unsafe fn` is an open question for upstream review; they are currently `safe`
/// because `BusAdapter` is `no_std`-firmware-only and the aliasing contract is
/// documented here.
#[cfg(feature = "transfer")]
impl BusAdapter {
    /// Queue an IN transfer (device→host) from `buf`. Zero-copy; `buf` must
    /// remain valid and unmodified until `poll_transfer` retires it.
    pub fn submit_write(
        &self,
        ep: usb_device::endpoint::EndpointAddress,
        buf: &[u8],
    ) -> usb_device::Result<()> {
        self.with_usb_mut(|usb| usb.ep_submit(ep, buf.as_ptr() as *mut u8, buf.len()))
    }

    /// Queue an OUT transfer (host→device) into `buf`. Zero-copy; `buf` must
    /// remain valid and unmodified until `poll_transfer` retires it.
    pub fn submit_read(
        &self,
        ep: usb_device::endpoint::EndpointAddress,
        buf: &mut [u8],
    ) -> usb_device::Result<()> {
        self.with_usb_mut(|usb| usb.ep_submit(ep, buf.as_mut_ptr(), buf.len()))
    }

    /// Retire the oldest completed transfer on `ep`, if any.
    ///
    /// Returns `Some(Ok(n))` when the oldest transfer completed with `n` bytes
    /// transferred, `Some(Err(_))` on a dTD error, or `None` if no transfer has
    /// completed yet.
    pub fn poll_transfer(
        &self,
        ep: usb_device::endpoint::EndpointAddress,
    ) -> Option<usb_device::Result<usize>> {
        self.with_usb_mut(|usb| usb.ep_poll_transfer(ep))
    }

    /// Number of queued (not yet retired) transfers on `ep`.
    pub fn pending_transfers(&self, ep: usb_device::endpoint::EndpointAddress) -> usize {
        self.with_usb(|usb| usb.ep_pending_transfers(ep))
    }

    /// Cancel every queued transfer on `ep`: flush the hardware prime and drop
    /// all software queue records (staging and zero-copy alike).
    ///
    /// For class-level reconfiguration of a shared endpoint — e.g. a
    /// SET_INTERFACE alternate-setting switch where a transfer primed by the
    /// previous alt setting would otherwise swallow the new alt's data. The
    /// bus layer never observes alt switches, so the class/firmware must call
    /// this explicitly when it changes which protocol owns an endpoint.
    /// No-op on EP0 and unallocated endpoints.
    pub fn cancel_transfers(&self, ep: usb_device::endpoint::EndpointAddress) {
        self.with_usb_mut(|usb| usb.ep_cancel_transfers(ep));
    }

    /// Configure eager packet read-ahead depth for an OUT endpoint.
    ///
    /// Call after `UsbDevice` configuration and before traffic. `depth` must be
    /// ≥ 1. Returns `Err(InvalidEndpoint)` for IN or control endpoints.
    pub fn set_packet_queue_depth(
        &self,
        ep: usb_device::endpoint::EndpointAddress,
        depth: usize,
    ) -> usb_device::Result<()> {
        self.with_usb_mut(|usb| usb.set_packet_queue_depth(ep, depth))
    }
}

impl UsbBus for BusAdapter {
    /// The USB hardware can guarantee that we set the status before we receive
    /// the status, and we're taking advantage of that. We expect this flag to
    /// result in a call to set_address before the status happens. This means
    /// that we can meet the timing requirements without help from software.
    ///
    /// It's not a quirk; it's a feature :)
    const QUIRK_SET_ADDRESS_BEFORE_STATUS: bool = true;

    fn alloc_ep(
        &mut self,
        ep_dir: UsbDirection,
        ep_addr: Option<EndpointAddress>,
        ep_type: EndpointType,
        max_packet_size: u16,
        _interval: u8,
    ) -> usb_device::Result<EndpointAddress> {
        self.with_usb_mut(|usb| {
            if let Some(addr) = ep_addr {
                if usb.is_allocated(addr) {
                    return Err(usb_device::UsbError::InvalidEndpoint);
                }
                let buffer = usb
                    .allocate_buffer(max_packet_size as usize)
                    .ok_or(usb_device::UsbError::EndpointMemoryOverflow)?;
                usb.allocate_ep(addr, buffer, ep_type);
                Ok(addr)
            } else {
                for idx in 1..crate::state::ENDPOINT_INDEX_COUNT {
                    let addr = EndpointAddress::from_parts(idx, ep_dir);
                    if usb.is_allocated(addr) {
                        continue;
                    }
                    let buffer = usb
                        .allocate_buffer(max_packet_size as usize)
                        .ok_or(usb_device::UsbError::EndpointMemoryOverflow)?;
                    usb.allocate_ep(addr, buffer, ep_type);
                    return Ok(addr);
                }
                Err(usb_device::UsbError::EndpointOverflow)
            }
        })
    }

    fn set_device_address(&self, addr: u8) {
        self.with_usb_mut(|usb| {
            usb.set_address(addr);
        });
    }

    fn enable(&mut self) {
        self.with_usb_mut(|usb| usb.attach());
    }

    fn reset(&self) {
        self.with_usb_mut(|usb| {
            usb.bus_reset();
        });
    }

    /// Simulate a disconnect so the host re-enumerates the device.
    ///
    /// Clears the controller's Run/Stop bit (removing the D+ pull-up), holds
    /// the disconnect for `DETACH_HOLD_CYCLES` so the host registers it, then
    /// re-asserts Run/Stop. The host then drives a bus reset, which flows
    /// through [`reset`](UsbBus::reset) and tears down any in-flight transfers;
    /// the device must be re-configured (call [`configure`](BusAdapter::configure)
    /// again) once it reaches the configured state, exactly as after any reset.
    ///
    /// The whole detach/hold/attach sequence runs inside the bus critical
    /// section, so it is not interrupted by USB ISR activity.
    fn force_reset(&self) -> usb_device::Result<()> {
        self.with_usb_mut(|usb| {
            usb.detach();
            cortex_m::asm::delay(DETACH_HOLD_CYCLES);
            usb.attach();
        });
        Ok(())
    }

    fn write(&self, ep_addr: EndpointAddress, buf: &[u8]) -> usb_device::Result<usize> {
        self.with_usb_mut(|usb| {
            if !usb.is_allocated(ep_addr) {
                return Err(usb_device::UsbError::InvalidEndpoint);
            }

            let written = if ep_addr.index() == 0 {
                usb.ctrl0_write(buf)
            } else {
                usb.ep_write(buf, ep_addr)
            }
            .inspect_err(|&_status| {
                // WouldBlock is the designed not-ready return on the hot path
                // (polled every loop) — keep it at TRACE so it cannot flood.
                if matches!(_status, usb_device::UsbError::WouldBlock) {
                    trace!(
                        "EP{=usize} {} STATUS WouldBlock",
                        ep_addr.index(),
                        ep_addr.direction()
                    );
                } else {
                    warn!(
                        "EP{=usize} {} STATUS {}",
                        ep_addr.index(),
                        ep_addr.direction(),
                        _status
                    );
                }
            })?;

            Ok(written)
        })
    }

    fn read(&self, ep_addr: EndpointAddress, buf: &mut [u8]) -> usb_device::Result<usize> {
        self.with_usb_mut(|usb| {
            if !usb.is_allocated(ep_addr) {
                return Err(usb_device::UsbError::InvalidEndpoint);
            }

            let read = if ep_addr.index() == 0 {
                usb.ctrl0_read(buf)
            } else {
                usb.ep_read(buf, ep_addr)
            }
            .inspect_err(|&_status| {
                // WouldBlock is the designed nothing-to-read return on the hot
                // path (polled every loop) — TRACE, not WARN (no flood).
                if matches!(_status, usb_device::UsbError::WouldBlock) {
                    trace!(
                        "EP{=usize} {} STATUS WouldBlock",
                        ep_addr.index(),
                        ep_addr.direction()
                    );
                } else {
                    warn!(
                        "EP{=usize} {} STATUS {}",
                        ep_addr.index(),
                        ep_addr.direction(),
                        _status
                    );
                }
            })?;

            Ok(read)
        })
    }

    fn set_stalled(&self, ep_addr: EndpointAddress, stalled: bool) {
        self.with_usb_mut(|usb| {
            if usb.is_allocated(ep_addr) {
                usb.ep_stall(stalled, ep_addr);
            }
        });
    }

    fn is_stalled(&self, ep_addr: EndpointAddress) -> bool {
        // The host can name any endpoint with GET_STATUS(Endpoint), allocated or not.
        self.with_usb(|usb| usb.is_allocated(ep_addr) && usb.is_ep_stalled(ep_addr))
    }

    fn suspend(&self) {
        // TODO
    }

    fn resume(&self) {
        // TODO
    }

    fn poll(&self) -> PollResult {
        self.with_usb_mut(|usb| usb.poll())
    }
}

// grcov exclude start
#[cfg(test)]
mod tests {
    use super::BusAdapter;
    use crate::driver::fake_driver;
    use core::cell::RefCell;
    use cortex_m::interrupt::{CriticalSection, Mutex};
    use usb_device::{
        UsbDirection,
        bus::UsbBus,
        endpoint::{EndpointAddress, EndpointType},
    };

    /// A `BusAdapter` over fake registers. It carries its own critical section,
    /// since `interrupt::free` has no host implementation.
    fn fake_bus() -> BusAdapter {
        BusAdapter {
            usb: Mutex::new(RefCell::new(fake_driver())),
            // Safety: the test owns the adapter and runs no interrupt handler.
            cs: Some(unsafe { CriticalSection::new() }),
        }
    }

    #[test]
    fn is_stalled_is_false_for_unallocated_endpoint() {
        // Given a bus with no endpoint allocated
        let bus = fake_bus();

        // When a host `GET_STATUS(Endpoint)` names an endpoint the device never
        // allocated, in any direction and at any index the request can carry
        for index in 0..16 {
            for direction in [UsbDirection::In, UsbDirection::Out] {
                let addr = EndpointAddress::from_parts(index, direction);

                // Then the bus answers instead of panicking
                assert!(
                    !bus.is_stalled(addr),
                    "unallocated EP{index} {direction:?} must not report a stall"
                );
            }
        }
    }

    #[test]
    fn is_stalled_follows_allocated_endpoint() {
        // Given an allocated bulk IN endpoint
        let mut bus = fake_bus();
        let addr = bus
            .alloc_ep(UsbDirection::In, None, EndpointType::Bulk, 64, 0)
            .expect("endpoint allocation");
        assert!(!bus.is_stalled(addr), "a fresh endpoint is not stalled");

        // When it is stalled, then unstalled
        bus.set_stalled(addr, true);
        let stalled = bus.is_stalled(addr);
        bus.set_stalled(addr, false);
        let unstalled = bus.is_stalled(addr);

        // Then the answer tracks the endpoint
        assert!(stalled, "a stalled endpoint reports a stall");
        assert!(!unstalled, "an unstalled endpoint reports none");
    }

    #[test]
    fn reset_data_toggle_reaches_the_endpoint_and_spares_set_stalled() {
        // Given an allocated bulk OUT endpoint
        let mut bus = fake_bus();
        let addr = bus
            .alloc_ep(UsbDirection::Out, None, EndpointType::Bulk, 64, 0)
            .expect("endpoint allocation");
        let rxr = |bus: &BusAdapter| bus.with_usb(|usb| usb.toggle_reset_bit(addr));

        // When a class unstalls it in the normal course of work
        bus.set_stalled(addr, true);
        bus.set_stalled(addr, false);

        // Then the toggle is untouched
        assert!(!rxr(&bus), "a routine unstall must not reset the toggle");

        // When firmware sees the host's CLEAR_FEATURE(ENDPOINT_HALT)
        bus.reset_data_toggle(addr);

        // Then it is reset
        assert!(rxr(&bus), "reset_data_toggle must reset the toggle");
    }

    #[test]
    fn reset_data_toggle_ignores_an_unallocated_endpoint() {
        // Given a bus with nothing allocated
        let bus = fake_bus();

        // When firmware names an endpoint the device never allocated
        for index in 0..16 {
            bus.reset_data_toggle(EndpointAddress::from_parts(index, UsbDirection::In));
        }

        // Then the call returns instead of panicking
    }
}
// grcov exclude stop
