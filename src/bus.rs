//! USB bus implementation

use crate::{endpoint::Status, USB};
use core::cell::RefCell;
use cortex_m::interrupt::{self, Mutex};
use imxrt_ral as ral;
use usb_device::{
    bus::{PollResult, UsbBus},
    endpoint::{EndpointAddress, EndpointType},
    UsbDirection,
};

pub struct Bus {
    usb: Mutex<RefCell<USB>>,
    endpoint_assigner: EndpointAssigner,
}

fn index(ep_addr: EndpointAddress) -> usize {
    (ep_addr.index() * 2) + (UsbDirection::In == ep_addr.direction()) as usize
}

impl Bus {
    /// Create a USB bus adapter from a `USB` object
    ///
    /// Make sure you've fully configured your USB device before wrapping it in `Bus`.
    pub fn new(usb: USB) -> Self {
        Bus {
            usb: Mutex::new(RefCell::new(usb)),
            endpoint_assigner: EndpointAssigner::new(),
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

    pub fn configure_endpoints(&self) {
        self.with_usb_mut(|usb| {
            for ep in usb.endpoints.iter_mut().flat_map(core::convert::identity) {
                ep.configure(&usb.usb);
                if UsbDirection::Out == ep.address().direction() {
                    let max_packet_len = ep.max_packet_len();
                    ep.schedule_transfer(&usb.usb, max_packet_len);
                }
            }
        });
    }
}

unsafe impl Send for crate::buffer::Allocator {}
unsafe impl Send for crate::Endpoint {}

impl UsbBus for Bus {
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
        let ep_addr = if let Some(ep_addr) = ep_addr {
            if !self
                .endpoint_assigner
                .assign_endpoint(ep_addr.index(), ep_addr.direction())
            {
                return Err(usb_device::UsbError::InvalidEndpoint);
            }
            ep_addr
        } else if let Some(ep_idx) = self.endpoint_assigner.assign_next_endpoint(ep_dir) {
            EndpointAddress::from_parts(ep_idx as usize, ep_dir)
        } else {
            return Err(usb_device::UsbError::EndpointOverflow);
        };

        self.with_usb_mut(|usb| {
            let buffer = match usb.buffer_allocator.allocate(max_packet_size as usize) {
                Some(buffer) => buffer,
                None => return Err(usb_device::UsbError::BufferOverflow),
            };

            let qh: &'static crate::qh::QH = &usb.qhs[index(ep_addr)];
            let td: &'static crate::td::TD = &usb.tds[index(ep_addr)];

            qh.set_max_packet_len(max_packet_size as usize);
            qh.set_zero_length_termination(false);
            qh.set_interrupt_on_setup(
                ep_type == EndpointType::Control && ep_addr.direction() == UsbDirection::Out,
            );

            td.set_terminate();
            td.clear_status();

            let mut ep: crate::Endpoint = unsafe {
                match ep_type {
                    EndpointType::Control => crate::endpoint::control(ep_addr, qh, td, buffer),
                    EndpointType::Bulk => crate::endpoint::bulk(ep_addr, qh, td, buffer),
                    EndpointType::Interrupt => crate::endpoint::interrupt(ep_addr, qh, td, buffer),
                    EndpointType::Isochronous => {
                        unimplemented!("No support for isochronous endpoints")
                    }
                }
            };
            debug!(
                "EP{} {:?} INITIALIZE {:?}",
                ep_addr.index(),
                ep_addr.direction(),
                ep_type
            );
            ep.initialize(&usb.usb);

            usb.endpoints[index(ep_addr)] = Some(ep);
            Ok(ep_addr)
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
            let ep = match &mut usb.endpoints[index(ep_addr)] {
                Some(ref mut ep) => ep,
                None => return Err(usb_device::UsbError::InvalidEndpoint),
            };
            trace!(
                "EP{} {:?} WRITE {}",
                ep_addr.index(),
                ep_addr.direction(),
                buf.len()
            );

            let status = ep.status();
            if status.contains(Status::DATA_BUS_ERROR | Status::TRANSACTION_ERROR | Status::HALTED)
            {
                warn!(
                    "EP{} {:?} STATUS {:?}",
                    ep_addr.index(),
                    ep_addr.direction(),
                    status
                );
                return Err(usb_device::UsbError::InvalidState);
            } else if status.contains(Status::ACTIVE) {
                warn!("EP{} {:?} ACTIVE", ep_addr.index(), ep_addr.direction());
                return Err(usb_device::UsbError::WouldBlock);
            }

            ep.clear_nack(&usb.usb);

            let written = ep.write(buf);
            ep.schedule_transfer(&usb.usb, written);

            Ok(written)
        })
    }

    fn read(&self, ep_addr: EndpointAddress, buf: &mut [u8]) -> usb_device::Result<usize> {
        self.with_usb_mut(|usb| {
            let ep = match &mut usb.endpoints[index(ep_addr)] {
                Some(ref mut ep) => ep,
                None => return Err(usb_device::UsbError::InvalidEndpoint),
            };

            let status = ep.status();
            if status.contains(Status::DATA_BUS_ERROR | Status::TRANSACTION_ERROR | Status::HALTED)
            {
                warn!(
                    "EP{} {:?} STATUS {:?}",
                    ep_addr.index(),
                    ep_addr.direction(),
                    status
                );
                return Err(usb_device::UsbError::InvalidState);
            } else if status.contains(Status::ACTIVE) {
                warn!("EP{} {:?} ACTIVE", ep_addr.index(), ep_addr.direction());
                return Err(usb_device::UsbError::WouldBlock);
            }

            ep.clear_complete(&usb.usb);
            ep.clear_nack(&usb.usb);

            if ep_addr.index() == 0 {
                // Do they want to read the setup data? Let's guess...
                if ep.has_setup(&usb.usb) && buf.len() >= 8 {
                    trace!("EP{} {:?} READ SETUP", ep_addr.index(), ep_addr.direction());

                    let setup = ep.read_setup(&usb.usb);
                    buf[..8].copy_from_slice(&setup.to_le_bytes());

                    return Ok(8);
                } else {
                    trace!(
                        "EP{} {:?} READ {}",
                        ep_addr.index(),
                        ep_addr.direction(),
                        buf.len()
                    );
                }
            }

            let read = ep.read(buf);
            let max_packet_len = ep.max_packet_len();
            ep.schedule_transfer(&usb.usb, max_packet_len);

            Ok(read)
        })
    }

    fn set_stalled(&self, ep_addr: EndpointAddress, stalled: bool) {
        self.with_usb_mut(|usb| {
            if let Some(ep) = &mut usb.endpoints[index(ep_addr)] {
                ep.set_stalled(&usb.usb, stalled);
            }
        });
    }

    fn is_stalled(&self, ep_addr: EndpointAddress) -> bool {
        self.with_usb(|usb| {
            if let Some(ep) = &usb.endpoints[index(ep_addr)] {
                ep.is_stalled(&usb.usb)
            } else {
                panic!("is_stalled called on an invalid endpoint")
            }
        })
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

struct EndpointAssigner {
    ep_out: u16,
    ep_in: u16,
}

impl EndpointAssigner {
    const fn new() -> Self {
        EndpointAssigner {
            ep_out: 0,
            ep_in: 0,
        }
    }
    /// Assigns a new endpoint (never EP0)
    fn assign_next_endpoint(&mut self, direction: UsbDirection) -> Option<u32> {
        let mask: &mut u16 = match direction {
            UsbDirection::Out => &mut self.ep_out,
            UsbDirection::In => &mut self.ep_in,
        };
        let offset = (*mask | 1).trailing_ones();
        if offset == 8 {
            None
        } else {
            *mask |= 1 << offset;
            Some(offset)
        }
    }
    /// Assigns a specific endpoint (the only way to assign EP0)
    fn assign_endpoint(&mut self, endpoint: usize, direction: UsbDirection) -> bool {
        let mask: &mut u16 = match direction {
            UsbDirection::Out => &mut self.ep_out,
            UsbDirection::In => &mut self.ep_in,
        };
        if *mask & (1 << endpoint) != 0 {
            false
        } else {
            *mask |= 1 << endpoint;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EndpointAssigner;
    use super::UsbDirection;
    #[test]
    fn endpoint_next_assignment() {
        let mut eps = EndpointAssigner::new();
        for idx in 1..8 {
            assert_eq!(eps.assign_next_endpoint(UsbDirection::Out), Some(idx));
        }

        assert_eq!(eps.assign_next_endpoint(UsbDirection::Out), None);

        for idx in 1..8 {
            assert_eq!(eps.assign_next_endpoint(UsbDirection::In), Some(idx));
        }

        assert_eq!(eps.assign_next_endpoint(UsbDirection::In), None);
    }

    #[test]
    fn endpoint_assignment() {
        let mut eps = EndpointAssigner::new();
        assert!(eps.assign_endpoint(3, UsbDirection::Out));
        for idx in 1..3 {
            assert_eq!(eps.assign_next_endpoint(UsbDirection::Out), Some(idx));
        }
        assert!(!eps.assign_endpoint(2, UsbDirection::Out));
        for idx in 4..8 {
            assert_eq!(eps.assign_next_endpoint(UsbDirection::Out), Some(idx));
        }
        assert_eq!(eps.assign_next_endpoint(UsbDirection::Out), None);
        assert!(eps.assign_endpoint(0, UsbDirection::Out));
        assert!(!eps.assign_endpoint(0, UsbDirection::Out));
    }
}
