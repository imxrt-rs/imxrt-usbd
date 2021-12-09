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

use super::driver::FullSpeed;
use crate::gpt;
use core::cell::RefCell;
use cortex_m::interrupt::{self, Mutex};
use usb_device::UsbError::WouldBlock;
use usb_device::{
    bus::{PollResult, UsbBus},
    endpoint::{EndpointAddress, EndpointType},
    UsbDirection,
};

/// A `UsbBus` implementation
///
/// The `BusAdapter` adapts the RAL instances, and exposes a `UsbBus` implementation.
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
/// use imxrt_usbd::full_speed::BusAdapter;
///
/// # struct Ps; use imxrt_usbd::Instance as Inst;
/// # unsafe impl imxrt_usbd::Peripherals for Ps { fn instance(&self) -> Inst { panic!() } }
/// static mut ENDPOINT_MEMORY: [u8; 1024] = [0; 1024];
///
/// // TODO initialize clocks...
///
/// let my_usb_peripherals = // Your Peripherals instance...
/// #   Ps;
/// let bus_adapter = BusAdapter::new(
///     my_usb_peripherals,
///     unsafe { &mut ENDPOINT_MEMORY }
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
pub struct BusAdapter {
    usb: Mutex<RefCell<FullSpeed>>,
}

impl BusAdapter {
    /// Create a USB bus adapter
    ///
    /// When this function returns, the `BusAdapter` has initialized the PHY and USB core peripherals.
    /// The adapter expects to own these two peripherals, along with the other peripherals required
    /// by the [`Peripherals`](crate::Peripherals) safety contract.
    ///
    /// You must also provide a region of memory that will used for endpoint I/O. The
    /// memory region will be partitioned for the endpoints, based on their requirements.
    ///
    /// You must ensure that no one else is using the endpoint memory!
    pub fn new<P: crate::Peripherals>(peripherals: P, buffer: &'static mut [u8]) -> Self {
        let mut usb = FullSpeed::new(peripherals);

        usb.initialize();
        usb.set_endpoint_memory(buffer);

        BusAdapter {
            usb: Mutex::new(RefCell::new(usb)),
        }
    }

    /// Enable (`true`) or disable (`false`) interrupts for this USB peripheral
    ///
    /// The interrupt causes are implementation specific. To handle the interrupt,
    /// call [`poll()`](BusAdapter::poll).
    pub fn set_interrupts(&self, interrupts: bool) {
        self.with_usb_mut(|usb| usb.set_interrupts(interrupts));
    }

    /// Interrupt-safe, immutable access to the USB peripheral
    fn with_usb<R>(&self, func: impl FnOnce(&FullSpeed) -> R) -> R {
        interrupt::free(|cs| {
            let usb = self.usb.borrow(cs);
            let usb = usb.borrow();
            func(&*usb)
        })
    }

    /// Interrupt-safe, mutable access to the USB peripheral
    fn with_usb_mut<R>(&self, func: impl FnOnce(&mut FullSpeed) -> R) -> R {
        interrupt::free(|cs| {
            let usb = self.usb.borrow(cs);
            let mut usb = usb.borrow_mut();
            func(&mut *usb)
        })
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
    /// `gpt` requires a critical section to guarantee a single
    /// mutable access to a USB GPT. See `cortex_m::interrupt::free`
    /// for more information.
    ///
    /// # Panics
    ///
    /// Panics if the GPT instance is already borrowed. This could happen
    /// if you call `borrow_gpt` again within the `func` callback.
    pub fn borrow_gpt<R>(
        &self,
        cs: &cortex_m::interrupt::CriticalSection,
        instance: gpt::Instance,
        func: impl FnOnce(&mut gpt::Gpt) -> R,
    ) -> R {
        let usb = self.usb.borrow(cs);
        usb.borrow_mut().gpt_mut(instance, func)
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
        self.maybe_write(ep_addr, || Ok(buf))
            .unwrap_or(Err(WouldBlock))
    }

    fn maybe_write<'a>(
        &self,
        ep_addr: EndpointAddress,
        producer: impl FnOnce() -> usb_device::Result<&'a [u8]>,
    ) -> Option<usb_device::Result<usize>> {
        self.with_usb_mut(|usb| {
            if !usb.is_allocated(ep_addr) {
                return Some(Err(usb_device::UsbError::InvalidEndpoint));
            }

            let written = if ep_addr.index() == 0 {
                usb.ctrl0_write(producer)
            } else {
                usb.ep_write(producer, ep_addr)
            };
            written.map(|result| {
                result.map_err(|status| {
                    warn!(
                        "EP{} {:?} STATUS {:?}",
                        ep_addr.index(),
                        ep_addr.direction(),
                        status
                    );
                    status
                })
            })
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
