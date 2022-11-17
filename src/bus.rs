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
    bus::{PollResult, UsbBus},
    endpoint::{EndpointAddress, EndpointType},
    UsbDirection,
};

pub use super::driver::Speed;

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
/// use imxrt_usbd::BusAdapter;
///
/// # struct Ps; use imxrt_usbd::Instance as Inst;
/// # unsafe impl imxrt_usbd::Peripherals for Ps { fn instance(&self) -> Inst { panic!() } }
/// static mut ENDPOINT_MEMORY: [u8; 1024] = [0; 1024];
/// static EP_STATE: imxrt_usbd::EndpointState = imxrt_usbd::EndpointState::max_endpoints();
///
/// // TODO initialize clocks...
///
/// let my_usb_peripherals = // Your Peripherals instance...
/// #   Ps;
/// let bus_adapter = BusAdapter::new(
///     my_usb_peripherals,
///     unsafe { &mut ENDPOINT_MEMORY },
///     &EP_STATE,
/// );
///
/// // Create the USB device...
/// use usb_device::prelude::*;
/// let bus_allocator = usb_device::bus::UsbBusAllocator::new(bus_adapter);
/// let mut device = UsbDeviceBuilder::new(&bus_allocator, UsbVidPid(0x5824, 0x27dd))
///     .product("imxrt-usbd")
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
    /// Panics if `state` has already been associated with another USB bus.
    pub fn new<P: crate::Peripherals, const EP_COUNT: usize>(
        peripherals: P,
        buffer: &'static mut [u8],
        state: &'static crate::state::EndpointState<EP_COUNT>,
    ) -> Self {
        Self::with_speed(peripherals, buffer, state, Speed::High)
    }

    /// Create a USB bus adapter with the given speed
    ///
    /// Specify [`Speed::LowFull`] to throttle the USB data rate.
    ///
    /// When this function returns, the `BusAdapter` has initialized the PHY and USB core peripherals.
    /// The adapter expects to own these two peripherals, along with the other peripherals required
    /// by the [`Peripherals`](crate::Peripherals) safety contract.
    ///
    /// You must also provide a region of memory that will used for endpoint I/O. The
    /// memory region will be partitioned for the endpoints, based on their requirements.
    ///
    /// You must ensure that no one else is using the endpoint memory!
    ///
    /// # Panics
    ///
    /// Panics if `state` has already been associated with another USB bus.
    pub fn with_speed<P: crate::Peripherals, const EP_COUNT: usize>(
        peripherals: P,
        buffer: &'static mut [u8],
        state: &'static crate::state::EndpointState<EP_COUNT>,
        speed: Speed,
    ) -> Self {
        Self::init(peripherals, buffer, state, speed, None)
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
    /// Panics if `state` has already been associated with another USB bus.
    pub unsafe fn without_critical_sections<P: crate::Peripherals, const EP_COUNT: usize>(
        peripherals: P,
        buffer: &'static mut [u8],
        state: &'static crate::state::EndpointState<EP_COUNT>,
        speed: Speed,
    ) -> Self {
        Self::init(
            peripherals,
            buffer,
            state,
            speed,
            Some(cortex_m::interrupt::CriticalSection::new()),
        )
    }

    fn init<P: crate::Peripherals, const EP_COUNT: usize>(
        peripherals: P,
        buffer: &'static mut [u8],
        state: &'static crate::state::EndpointState<EP_COUNT>,
        speed: Speed,
        cs: Option<cortex_m::interrupt::CriticalSection>,
    ) -> Self {
        let mut usb = Driver::new(peripherals, state);

        usb.initialize(speed);
        usb.set_endpoint_memory(buffer);

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
                for idx in 1..8 {
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

    fn write(&self, ep_addr: EndpointAddress, buf: &[u8]) -> usb_device::Result<usize> {
        self.with_usb_mut(|usb| {
            if !usb.is_allocated(ep_addr) {
                return Err(usb_device::UsbError::InvalidEndpoint);
            }

            // Keep map_err if warn! is compiled out.
            #[allow(clippy::map_identity)]
            let written = if ep_addr.index() == 0 {
                usb.ctrl0_write(buf)
            } else {
                usb.ep_write(buf, ep_addr)
            }
            .map_err(|status| {
                warn!(
                    "EP{} {:?} STATUS {:?}",
                    ep_addr.index(),
                    ep_addr.direction(),
                    status
                );
                status
            })?;

            Ok(written)
        })
    }

    fn read(&self, ep_addr: EndpointAddress, buf: &mut [u8]) -> usb_device::Result<usize> {
        self.with_usb_mut(|usb| {
            if !usb.is_allocated(ep_addr) {
                return Err(usb_device::UsbError::InvalidEndpoint);
            }

            // Keep map_err if warn! is compiled out.
            #[allow(clippy::map_identity)]
            let read = if ep_addr.index() == 0 {
                usb.ctrl0_read(buf)
            } else {
                usb.ep_read(buf, ep_addr)
            }
            .map_err(|status| {
                warn!(
                    "EP{} {:?} STATUS {:?}",
                    ep_addr.index(),
                    ep_addr.direction(),
                    status
                );
                status
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
        self.with_usb(|usb| usb.is_ep_stalled(ep_addr))
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
