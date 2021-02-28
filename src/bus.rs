//! USB bus implementation

use crate::ral;
use crate::USB;
use core::cell::RefCell;
use core::convert::TryInto;
use cortex_m::interrupt::{self, Mutex};
use usb_device::{
    bus::{PollResult, UsbBus},
    endpoint::{EndpointAddress, EndpointType},
    UsbDirection,
};

/// A `UsbBus` implementation
///
/// After you set up your [`USB`](crate::USB) handle, you can create a `BusAdapter`
/// and supply that as the implementation of your `usb-device` device.
///
/// # Requirements
///
/// When you build your final `usb-device`, you must set the endpoint 0 max packet
/// size to 64 bytes. See the `UsbDeviceBuilder::max_packet_size_0` for more information.
/// Failure to increase the control endpoint max packet size will result in a USB device
/// that cannot communicate with the host.
///
/// Additionally, before polling for USB class traffic, you must call [`configure()`](BusAdapter::configure())
/// *after* your device has been configured. This can be accomplished by polling the USB
/// device and checking its state until it's been configured. Once configured, use `UsbDevice::bus()`
/// to access the i.MX RT `BusAdapter`, and call `configure()`. You should only do this once.
/// after that, you may poll for class traffic.
///
/// # Example
///
/// This example shows you how to create a `BusAdapter`, build a simple USB device, and
/// prepare the device for class traffic. It assumes that you're familiar with how
/// to initialize a [`USB`](crate::USB) driver.
///
/// Note that this example does not demonstrate USB class allocation or polling. See
/// your USB class' documentation for details.
///
/// ```no_run
/// # use imxrt_usb::USB;
/// # use imxrt_ral::{usb, usbphy};
/// // From the USB example...
/// let usb = USB::new(
///     usb::USB1::take().unwrap(),
///     usbphy::USBPHY1::take().unwrap(),
/// );
///
/// // Construct the bus...
/// use imxrt_usb::BusAdapter;
/// let bus_adapter = BusAdapter::new(usb);
///
/// // Create the USB device...
/// use usb_device::prelude::*;
/// let bus_allocator = usb_device::bus::UsbBusAllocator::new(bus_adapter);
/// let mut device = UsbDeviceBuilder::new(&bus_allocator, UsbVidPid(0x5824, 0x27dd))
///     .product("imxrt-usb")
///     .max_packet_size_0(64) // <---- Set the control OUT/IN max packet size to 64
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
    usb: Mutex<RefCell<USB>>,
}

impl BusAdapter {
    /// Create a USB bus adapter from a `USB` object
    ///
    /// Make sure you've fully configured your USB device before wrapping it in the adapter.
    pub fn new(usb: USB) -> Self {
        BusAdapter {
            usb: Mutex::new(RefCell::new(usb)),
        }
    }
    /// Interrupt-safe, immutable access to the USB peripheral
    fn with_usb<R>(&self, func: impl FnOnce(&USB) -> R) -> R {
        interrupt::free(|cs| {
            let usb = self.usb.borrow(cs);
            let usb = usb.borrow();
            func(&*usb)
        })
    }
    /// Interrupt-safe, mutable access to the USB peripheral
    fn with_usb_mut<R>(&self, func: impl FnOnce(&mut USB) -> R) -> R {
        interrupt::free(|cs| {
            let usb = self.usb.borrow(cs);
            let mut usb = usb.borrow_mut();
            func(&mut *usb)
        })
    }

    /// Apply device configurations, and perform other post-configuration actions
    ///
    /// You must invoke this once, and only after your device has been configured. See
    /// the top-level example for how this could be achieved.
    pub fn configure(&self) {
        self.with_usb_mut(|usb| {
            usb.enable_endpoints();
            debug!("CONFIGURED");
        });
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
            debug!("ADDRESS {}", addr);
        });
    }

    fn enable(&mut self) {
        self.with_usb_mut(|usb| usb.attach());
    }

    fn reset(&self) {
        self.with_usb_mut(|usb| {
            usb.bus_reset();
            debug!("RESET");
        });
    }

    fn write(&self, ep_addr: EndpointAddress, buf: &[u8]) -> usb_device::Result<usize> {
        self.with_usb_mut(|usb| {
            if !usb.is_allocated(ep_addr) {
                return Err(usb_device::UsbError::InvalidEndpoint);
            }

            debug!(
                "EP{} {:?} WRITE {}",
                ep_addr.index(),
                ep_addr.direction(),
                buf.len()
            );

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
                status.try_into().unwrap()
            })?;

            Ok(written)
        })
    }

    fn read(&self, ep_addr: EndpointAddress, buf: &mut [u8]) -> usb_device::Result<usize> {
        self.with_usb_mut(|usb| {
            if !usb.is_allocated(ep_addr) {
                return Err(usb_device::UsbError::InvalidEndpoint);
            }

            debug!(
                "EP{} {:?} READ {}",
                ep_addr.index(),
                ep_addr.direction(),
                buf.len()
            );

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
                status.try_into().unwrap()
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
        unimplemented!("Nothing to do; not signaling suspend / resume from poll")
    }

    fn resume(&self) {
        unimplemented!("Nothing to do; not signaling suspend / resume from poll")
    }

    fn poll(&self) -> PollResult {
        self.with_usb_mut(|usb| {
            let usbsts = ral::read_reg!(ral::usb, usb.usb, USBSTS);
            ral::write_reg!(ral::usb, usb.usb, USBSTS, usbsts);

            use ral::usb::USBSTS;
            if usbsts & USBSTS::URI::mask != 0 {
                PollResult::Reset
            } else if usbsts & USBSTS::UI::mask != 0 {
                trace!("========================");
                trace!(
                    "ENDPTCOMPLETE {:08X}",
                    ral::read_reg!(ral::usb, usb.usb, ENDPTCOMPLETE)
                );
                trace!(
                    "ENDPTSETUPSTAT {:08X}",
                    ral::read_reg!(ral::usb, usb.usb, ENDPTSETUPSTAT)
                );
                // Note: could be complete in one register read, but this is a little
                // easier to read...
                let ep_out = ral::read_reg!(ral::usb, usb.usb, ENDPTCOMPLETE, ERCE);

                let ep_in_complete = ral::read_reg!(ral::usb, usb.usb, ENDPTCOMPLETE, ETCE);
                ral::write_reg!(ral::usb, usb.usb, ENDPTCOMPLETE, ETCE: ep_in_complete);

                let ep_setup = ral::read_reg!(ral::usb, usb.usb, ENDPTSETUPSTAT) as u16;

                PollResult::Data {
                    ep_out: ep_out as u16,
                    ep_in_complete: ep_in_complete as u16,
                    ep_setup,
                }
            } else {
                PollResult::None
            }
        })
    }
}
