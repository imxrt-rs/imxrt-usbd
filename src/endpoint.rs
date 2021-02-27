use crate::{buffer::Buffer, qh::QH, ral, td::TD};
use usb_device::{endpoint::EndpointAddress, UsbDirection};

/// The RAL API requires us to treat all endpoint control registers as unique.
/// We can make it a little easier with this function, the `EndptCtrl` type,
/// and the helper module.
mod endpoint_control {
    use imxrt_ral as ral;

    #[allow(non_snake_case)]
    pub struct EndptCtrl<'a> {
        pub ENDPTCTRL: &'a ral::RWRegister<u32>,
    }

    #[allow(non_snake_case)]
    pub mod ENDPTCTRL {
        pub use imxrt_ral::usb::ENDPTCTRL1::*;
    }

    pub fn register<'a>(usb: &'a ral::usb::Instance, endpoint: usize) -> EndptCtrl<'a> {
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
}

/// Endpoint transfer status
pub type Status = crate::td::Status;

impl From<Status> for usb_device::UsbError {
    fn from(status: Status) -> Self {
        // Keep this implementation in sync with any changes in
        // Endpoint::check_status()
        if status.contains(Status::DATA_BUS_ERROR | Status::TRANSACTION_ERROR | Status::HALTED) {
            usb_device::UsbError::InvalidState
        } else if status.contains(Status::ACTIVE) {
            usb_device::UsbError::WouldBlock
        } else {
            panic!("Unhandled Status => UsbError conversion");
        }
    }
}

/// A USB endpoint
pub struct Endpoint {
    address: EndpointAddress,
    qh: &'static mut QH,
    td: &'static mut TD,
    buffer: Buffer,
}

impl Endpoint {
    pub fn new(
        address: EndpointAddress,
        qh: &'static mut QH,
        td: &'static mut TD,
        buffer: Buffer,
    ) -> Self {
        Endpoint {
            address,
            qh,
            td,
            buffer,
        }
    }

    /// Check for any transfer status, which is signaled through
    /// an error
    pub fn check_status(&self) -> Result<(), Status> {
        let status = self.td.status();
        if status.is_empty() {
            Ok(())
        } else {
            Err(status)
        }
    }

    /// Initialize the endpoint, should be called soon after it's assigned
    pub fn initialize(
        &mut self,
        usb: &ral::usb::Instance,
        ep_type: usb_device::endpoint::EndpointType,
    ) {
        if self.address.index() != 0 {
            let endptctrl = endpoint_control::register(usb, self.address.index());
            match self.address.direction() {
                UsbDirection::In => {
                    ral::modify_reg!(endpoint_control, &endptctrl, ENDPTCTRL, TXE: 0, TXT: ep_type as u32)
                }
                UsbDirection::Out => {
                    ral::modify_reg!(endpoint_control, &endptctrl, ENDPTCTRL, RXE: 0, RXT: ep_type as u32)
                }
            }
        }
    }

    /// Returns the endpoint address
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
        self.buffer.volatile_read(&mut buffer[..size])
    }

    /// Write `buffer` to the endpoint buffer
    ///
    /// Returns the number of bytes written from `buffer`, which is constrained
    /// by the max packet length.
    pub fn write(&mut self, buffer: &[u8]) -> usize {
        let size = self.qh.max_packet_len().min(buffer.len());
        self.buffer.volatile_write(&buffer[..size])
    }

    /// Clear the complete bit for this endpoint
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
        self.td.set_buffer(self.buffer.as_ptr_mut(), size);
        self.td.set_interrupt_on_complete(true);
        self.td.set_active();

        self.qh.overlay_mut().set_next(self.td);
        self.qh.overlay_mut().clear_status();

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

    /// Stall or unstall the endpoint
    pub fn set_stalled(&mut self, usb: &ral::usb::Instance, stall: bool) {
        let endptctrl = endpoint_control::register(usb, self.address.index());

        match self.address.direction() {
            UsbDirection::In => {
                ral::modify_reg!(endpoint_control, &endptctrl, ENDPTCTRL, TXS: stall as u32)
            }
            UsbDirection::Out => {
                ral::modify_reg!(endpoint_control, &endptctrl, ENDPTCTRL, RXS: stall as u32)
            }
        }
    }

    /// Indicates if the endpoint is stalled
    pub fn is_stalled(&self, usb: &ral::usb::Instance) -> bool {
        let endptctrl = endpoint_control::register(usb, self.address.index());

        match self.address.direction() {
            UsbDirection::In => ral::read_reg!(endpoint_control, &endptctrl, ENDPTCTRL, TXS == 1),
            UsbDirection::Out => ral::read_reg!(endpoint_control, &endptctrl, ENDPTCTRL, RXS == 1),
        }
    }

    /// Enable the endpoint
    ///
    /// This should be called only after the USB device has been configured.
    pub fn enable(&mut self, usb: &ral::usb::Instance) {
        // EP0 is always enabled
        if self.address.index() != 0 {
            let endptctrl = endpoint_control::register(usb, self.address.index());
            match self.address.direction() {
                UsbDirection::In => {
                    ral::modify_reg!(endpoint_control, &endptctrl, ENDPTCTRL, TXE: 1)
                }
                UsbDirection::Out => {
                    ral::modify_reg!(endpoint_control, &endptctrl, ENDPTCTRL, RXE: 1)
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

    /// Flush the endpoint, which could cancel pending transfers
    ///
    /// Blocks until the flush is complete.
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
