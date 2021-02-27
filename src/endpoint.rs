use crate::{qh::QH, ral, td::TD};
use core::ptr::NonNull;
use usb_device::{endpoint::EndpointAddress, UsbDirection};

fn endpoint_control_register<'a>(usb: &'a ral::usb::Instance, endpoint: usize) -> EndptCtrl<'a> {
    EndptCtrl {
        ENDPTCTRL: match endpoint {
            0 => &usb.ENDPTCTRL0,
            1 => &usb.ENDPTCTRL1,
            2 => &usb.ENDPTCTRL2,
            3 => &usb.ENDPTCTRL3,
            4 => &usb.ENDPTCTRL4,
            5 => &usb.ENDPTCTRL5,
            6 => &usb.ENDPTCTRL6,
            7 => &usb.ENDPTCTRL7,
            _ => unreachable!("ENDPTCTRL register {} doesn't exist", endpoint),
        },
    }
}

#[allow(non_snake_case)]
struct EndptCtrl<'a> {
    ENDPTCTRL: &'a ral::RWRegister<u32>,
}

#[allow(non_snake_case)]
mod ENDPTCTRL {
    pub use imxrt_ral::usb::ENDPTCTRL1::*;
}

pub type Status = crate::td::Status;

#[derive(Clone, Copy)]
#[repr(u32)]
enum Kind {
    Control = 0,
    // Isochronous = 1,
    // Not implemented, no support in usb_device ecosystem
    Bulk = 2,
    Interrupt = 3,
}

/// A USB endpoint
pub struct Endpoint {
    address: EndpointAddress,
    kind: Kind,
    qh: &'static QH,
    td: &'static TD,
    buffer: *mut u8,
}

/// Allocates a control endpoint that operates using the queue head, transfer descriptor,
/// and buffer.
///
/// Expects both the queue head and transfer descriptor to be initialized. Specifically,
/// queue head should describe a max packet length.
///
/// # Safety
///
/// All of the queue head, transfer descriptor, and buffer must only be used by this
/// endpoint. `buffer` must point to an allocation that's at least as large as the
/// queue head's max packet length. `buffer` must outlive the endpoint.
pub unsafe fn control(
    address: EndpointAddress,
    qh: &'static QH,
    td: &'static TD,
    buffer: NonNull<u8>,
) -> Endpoint {
    Endpoint::new(address, Kind::Control, qh, td, buffer)
}

/// Allocates a bulk endpoint that operates using the queue head, transfer descriptor,
/// and buffer. The endpoint address is 0.
///
/// Expects both the queue head and transfer descriptor to be initialized. Specifically,
/// queue head should describe a max packet length.
///
/// # Safety
///
/// All of the queue head, transfer descriptor, and buffer must only be used by this
/// endpoint. `buffer` must point to an allocation that's at least as large as the
/// queue head's max packet length. `buffer` must outlive the endpoint.
pub unsafe fn bulk(
    address: EndpointAddress,
    qh: &'static QH,
    td: &'static TD,
    buffer: NonNull<u8>,
) -> Endpoint {
    Endpoint::new(address, Kind::Bulk, qh, td, buffer)
}

/// Allocates an interrupt endpoint that operates using the queue head, transfer descriptor,
/// and buffer. The endpoint address is 0.
///
/// Expects both the queue head and transfer descriptor to be initialized. Specifically,
/// queue head should describe a max packet length.
///
/// # Safety
///
/// All of the queue head, transfer descriptor, and buffer must only be used by this
/// endpoint. `buffer` must point to an allocation that's at least as large as the
/// queue head's max packet length. `buffer` must outlive the endpoint.
pub unsafe fn interrupt(
    address: EndpointAddress,
    qh: &'static QH,
    td: &'static TD,
    buffer: NonNull<u8>,
) -> Endpoint {
    Endpoint::new(address, Kind::Interrupt, qh, td, buffer)
}

impl Endpoint {
    const unsafe fn new(
        address: EndpointAddress,
        kind: Kind,
        qh: &'static QH,
        td: &'static TD,
        buffer: NonNull<u8>,
    ) -> Self {
        Endpoint {
            address,
            kind,
            qh,
            td,
            buffer: buffer.as_ptr(),
        }
    }

    pub fn initialize(&mut self, usb: &ral::usb::Instance) {
        if self.address.index() != 0 {
            let endptctrl = endpoint_control_register(usb, self.address.index());
            match self.address.direction() {
                UsbDirection::In => {
                    ral::modify_reg!(self, &endptctrl, ENDPTCTRL, TXE: 0, TXT: Kind::Bulk as u32)
                }
                UsbDirection::Out => {
                    ral::modify_reg!(self, &endptctrl, ENDPTCTRL, RXE: 0, RXT: Kind::Bulk as u32)
                }
            }
        }
    }

    pub fn address(&self) -> EndpointAddress {
        self.address
    }

    /// Returns the maximum packet length supported by this endpoint
    pub fn max_packet_len(&self) -> usize {
        self.qh.max_packet_len()
    }

    /// Indicates if this endpoint has received setup data
    pub fn has_setup(&self, usb: &ral::usb::Instance) -> bool {
        ral::read_reg!(ral::usb, usb, ENDPTSETUPSTAT) & (1 << self.address.index()) != 0
    }

    /// Read the setup buffer from this endpoint
    ///
    /// This is only meaningful for a control OUT endpoint.
    pub fn read_setup(&mut self, usb: &ral::usb::Instance) -> u64 {
        // Reference manual isn't really clear on whe we should clear the ENDPTSETUPSTAT
        // bit...
        //
        // - section "Control Endpoint Operational Model" says that we should clear it
        //   *before* attempting to read the setup buffer, but
        // - section "Operational Model For Setup Transfers" says to do it *after*
        //   we read the setup buffer
        //
        // We're going with the "before" approach here. (Reference manual is iMXRT1060, rev2)
        ral::write_reg!(ral::usb, usb, ENDPTSETUPSTAT, 1 << self.address.index());
        loop {
            ral::modify_reg!(ral::usb, usb, USBCMD, SUTW: 1);
            let setup = self.qh.setup();
            if ral::read_reg!(ral::usb, usb, USBCMD, SUTW == 1) {
                ral::modify_reg!(ral::usb, usb, USBCMD, SUTW: 0);
                return setup;
            }
        }
    }

    /// Read data from the endpoint into `buffer`
    ///
    /// Returns the number of bytes read into `buffer`, which is constrained by the
    /// max packet length, and the number of bytes received in the last transfer.
    pub fn read(&mut self, buffer: &mut [u8]) -> usize {
        let size = self
            .qh
            .max_packet_len()
            .min(buffer.len())
            .min(self.td.bytes_transferred());
        buffer
            .iter_mut()
            .take(size)
            .fold(self.buffer, |src, dst| unsafe {
                *dst = src.read_volatile();
                src.add(1)
            });
        size
    }

    /// Write `buffer` to the endpoint buffer
    ///
    /// Returns the number of bytes written from `buffer`, which is constrained
    /// by the max packet length.
    pub fn write(&mut self, buffer: &[u8]) -> usize {
        let size = self.qh.max_packet_len().min(buffer.len());
        buffer
            .iter()
            .take(size)
            .fold(self.buffer, |dst, src| unsafe {
                dst.write_volatile(*src);
                dst.add(1)
            });
        size
    }

    pub fn clear_complete(&mut self, usb: &ral::usb::Instance) {
        match self.address.direction() {
            UsbDirection::In => {
                ral::write_reg!(ral::usb, usb, ENDPTCOMPLETE, ETCE: 1 << self.address.index())
            }
            UsbDirection::Out => {
                ral::write_reg!(ral::usb, usb, ENDPTCOMPLETE, ERCE: 1 << self.address.index())
            }
        }
    }

    /// Schedule a transfer of `size` bytes from the endpoint buffer
    ///
    /// Caller should check to see if there is an active transfer, or if the previous
    /// transfer resulted in an error or halt.
    pub fn schedule_transfer(&mut self, usb: &ral::usb::Instance, size: usize) {
        self.td.set_terminate();
        self.td.set_buffer(self.buffer, size);
        self.td.set_interrupt_on_complete(true);
        self.td.set_active();

        self.qh.overlay().set_next(self.td);
        self.qh.overlay().clear_status();

        match self.address.direction() {
            UsbDirection::In => {
                ral::write_reg!(ral::usb, usb, ENDPTPRIME, PETB: 1 << self.address.index())
            }
            UsbDirection::Out => {
                ral::write_reg!(ral::usb, usb, ENDPTPRIME, PERB: 1 << self.address.index())
            }
        }
        while ral::read_reg!(ral::usb, usb, ENDPTPRIME) != 0 {}
    }

    pub fn status(&self) -> Status {
        self.td.status()
    }

    pub fn set_stalled(&mut self, usb: &ral::usb::Instance, stall: bool) {
        let endptctrl = endpoint_control_register(usb, self.address.index());

        match self.address.direction() {
            UsbDirection::In => ral::modify_reg!(self, &endptctrl, ENDPTCTRL, TXS: stall as u32),
            UsbDirection::Out => ral::modify_reg!(self, &endptctrl, ENDPTCTRL, RXS: stall as u32),
        }
    }

    pub fn is_stalled(&self, usb: &ral::usb::Instance) -> bool {
        let endptctrl = endpoint_control_register(usb, self.address.index());

        match self.address.direction() {
            UsbDirection::In => ral::read_reg!(self, &endptctrl, ENDPTCTRL, TXS == 1),
            UsbDirection::Out => ral::read_reg!(self, &endptctrl, ENDPTCTRL, RXS == 1),
        }
    }

    /// Configure the endpoint
    ///
    /// This should be called only after the USB device has been configured.
    pub fn configure(&mut self, usb: &ral::usb::Instance) {
        if self.address.index() != 0 {
            let endptctrl = endpoint_control_register(usb, self.address.index());
            match self.address.direction() {
                UsbDirection::In => {
                    ral::modify_reg!(self, &endptctrl, ENDPTCTRL, TXE: 1, TXT: self.kind as u32)
                }
                UsbDirection::Out => {
                    ral::modify_reg!(self, &endptctrl, ENDPTCTRL, RXE: 1, RXT: self.kind as u32)
                }
            }
        }
    }

    /// Clear the NACK bit for this endpoint
    pub fn clear_nack(&mut self, usb: &ral::usb::Instance) {
        match self.address.direction() {
            UsbDirection::In => {
                ral::write_reg!(ral::usb, usb, ENDPTNAK, EPTN: 1 << self.address.index())
            }
            UsbDirection::Out => {
                ral::write_reg!(ral::usb, usb, ENDPTNAK, EPRN: 1 << self.address.index())
            }
        }
    }

    pub fn flush(&mut self, usb: &ral::usb::Instance) {
        match self.address.direction() {
            UsbDirection::In => {
                ral::write_reg!(ral::usb, usb, ENDPTFLUSH, FETB: 1 << self.address.index())
            }
            UsbDirection::Out => {
                ral::write_reg!(ral::usb, usb, ENDPTFLUSH, FERB: 1 << self.address.index())
            }
        }
        while ral::read_reg!(ral::usb, usb, ENDPTFLUSH) != 0 {}
    }
}
