use endpoint_trait::{
    endpoint::{EndpointAddress, EndpointConfig, EndpointType, OutPacketType},
    usbcore::{UsbEndpoint, UsbEndpointIn, UsbEndpointOut},
    Result, UsbDirection, UsbError,
};

use crate::{
    buffer::Buffer,
    qh::QH,
    ral,
    td::{Status, TD},
};

pub struct Endpoint {
    usb: &'static ral::usb::RegisterBlock,
    address: EndpointAddress,
    qh: &'static mut QH,
    td: &'static mut TD,
    buffer: Buffer,
}

impl Endpoint {
    pub fn new(
        usb: &'static ral::usb::RegisterBlock,
        address: EndpointAddress,
        qh: &'static mut QH,
        td: &'static mut TD,
        buffer: Buffer,
    ) -> Self {
        let max_packet_size = buffer.len();
        qh.set_max_packet_len(max_packet_size);
        qh.set_zero_length_termination(false);

        td.set_terminate();
        td.clear_status();
        Endpoint {
            usb,
            address,
            qh,
            td,
            buffer,
        }
    }

    /// Indicates if this is the control endpoint
    fn is_control(&self) -> bool {
        self.address.number() == 0
    }

    /// Read data from the endpoint into `buffer`
    ///
    /// Returns the number of bytes read into `buffer`, which is constrained by the
    /// max packet length, and the number of bytes received in the last transfer.
    fn read(&mut self, buffer: &mut [u8]) -> usize {
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
    fn write(&mut self, buffer: &[u8]) -> usize {
        let size = self.qh.max_packet_len().min(buffer.len());
        let written = self.buffer.volatile_write(&buffer[..size]);
        self.buffer.clean_invalidate_dcache(size);
        written
    }

    /// Check for any transfer status, which is signaled through
    /// an error
    fn check_errors(&self) -> Result<()> {
        let status = self.td.status();
        if status.contains(Status::TRANSACTION_ERROR)
            | status.contains(Status::DATA_BUS_ERROR)
            | status.contains(Status::HALTED)
        {
            Err(UsbError::Platform)
        } else {
            Ok(())
        }
    }

    /// Indicates if the transfer descriptor is active
    fn is_primed(&self) -> bool {
        (match self.address.direction() {
            UsbDirection::In => ral::read_reg!(ral::usb, self.usb, ENDPTSTAT, ETBR),
            UsbDirection::Out => ral::read_reg!(ral::usb, self.usb, ENDPTSTAT, ERBR),
        } & (1 << self.address.number()))
            != 0
    }

    /// Indicate if there's an active transfer
    fn is_active(&self) -> bool {
        self.td.status().contains(Status::ACTIVE)
    }

    /// Schedule a transfer of `size` bytes from the endpoint buffer
    ///
    /// Caller should check to see if there is an active transfer, or if the previous
    /// transfer resulted in an error or halt.
    fn schedule_transfer(&mut self, size: usize) {
        self.td.set_terminate();
        self.td.set_buffer(self.buffer.as_ptr_mut(), size);
        self.td.set_interrupt_on_complete(true);
        self.td.set_active();
        self.td.clean_invalidate_dcache();

        self.qh.overlay_mut().set_next(self.td);
        self.qh.overlay_mut().clear_status();
        self.qh.clean_invalidate_dcache();

        match self.address.direction() {
            UsbDirection::In => {
                ral::write_reg!(ral::usb, &self.usb, ENDPTPRIME, PETB: 1 << self.address.number())
            }
            UsbDirection::Out => {
                ral::write_reg!(ral::usb, &self.usb, ENDPTPRIME, PERB: 1 << self.address.number())
            }
        }
        while ral::read_reg!(ral::usb, &self.usb, ENDPTPRIME) != 0 {}
    }

    /// Clear the NACK bit for this endpoint
    fn clear_nack(&mut self) {
        match self.address.direction() {
            UsbDirection::In => {
                ral::write_reg!(ral::usb, &self.usb, ENDPTNAK, EPTN: 1 << self.address.number())
            }
            UsbDirection::Out => {
                ral::write_reg!(ral::usb, &self.usb, ENDPTNAK, EPRN: 1 << self.address.number())
            }
        }
    }

    /// Indicates if this endpoint has received setup data
    fn has_setup(&self) -> bool {
        ral::read_reg!(ral::usb, &self.usb, ENDPTSETUPSTAT) & (1 << self.address.number()) != 0
    }

    /// Read the setup buffer from this endpoint
    ///
    /// This is only meaningful for a control OUT endpoint.
    fn read_setup(&mut self) -> u64 {
        // Reference manual isn't really clear on whe we should clear the ENDPTSETUPSTAT
        // bit...
        //
        // - section "Control Endpoint Operational Model" says that we should clear it
        //   *before* attempting to read the setup buffer, but
        // - section "Operational Model For Setup Transfers" says to do it *after*
        //   we read the setup buffer
        //
        // We're going with the "before" approach here. (Reference manual is iMXRT1060, rev2)
        ral::write_reg!(
            ral::usb,
            &self.usb,
            ENDPTSETUPSTAT,
            1 << self.address.number()
        );
        loop {
            ral::modify_reg!(ral::usb, &self.usb, USBCMD, SUTW: 1);
            let setup = self.qh.setup();
            if ral::read_reg!(ral::usb, &self.usb, USBCMD, SUTW == 1) {
                ral::modify_reg!(ral::usb, &self.usb, USBCMD, SUTW: 0);
                return setup;
            }
        }
    }

    /// Returns the maximum packet length supported by this endpoint
    fn max_packet_len(&self) -> usize {
        self.qh.max_packet_len()
    }

    /// Clear the complete bit for this endpoint
    fn clear_complete(&mut self) {
        match self.address.direction() {
            UsbDirection::In => {
                ral::write_reg!(ral::usb, &self.usb, ENDPTCOMPLETE, ETCE: 1 << self.address.number())
            }
            UsbDirection::Out => {
                ral::write_reg!(ral::usb, &self.usb, ENDPTCOMPLETE, ERCE: 1 << self.address.number())
            }
        }
    }
}

impl UsbEndpoint for Endpoint {
    fn address(&self) -> EndpointAddress {
        self.address
    }

    unsafe fn enable(&mut self, config: &EndpointConfig) -> Result<()> {
        if config.max_packet_size() as usize > self.buffer.len() {
            return Err(UsbError::EndpointMemoryOverflow);
        } else if self.is_control() && config.ep_type() != EndpointType::Control {
            return Err(UsbError::InvalidEndpoint);
        }

        self.qh.set_interrupt_on_setup(
            config.ep_type() == EndpointType::Control
                && self.address.direction() == UsbDirection::Out,
        );

        if !self.is_control() {
            let endptctrl =
                ral::endpoint_control::register(&self.usb, self.address.number().into());
            match self.address.direction() {
                UsbDirection::In => {
                    ral::modify_reg!(ral::endpoint_control, &endptctrl, ENDPTCTRL, TXE: 1, TXR: 1, TXT: config.ep_type() as u32)
                }
                UsbDirection::Out => {
                    ral::modify_reg!(ral::endpoint_control, &endptctrl, ENDPTCTRL, RXE: 1, RXR: 1, RXT: config.ep_type() as u32)
                }
            }

            if self.address.direction() == UsbDirection::Out {
                let max_packet_len = self.max_packet_len();
                self.schedule_transfer(max_packet_len);
            }
        }

        Ok(())
    }
    fn disable(&mut self) -> Result<()> {
        if !self.is_control() {
            let endptctrl =
                ral::endpoint_control::register(&self.usb, self.address.number().into());
            match self.address.direction() {
                UsbDirection::In => {
                    ral::modify_reg!(ral::endpoint_control, &endptctrl, ENDPTCTRL, TXE: 0, TXT: EndpointType::Bulk as u32)
                }
                UsbDirection::Out => {
                    ral::modify_reg!(ral::endpoint_control, &endptctrl, ENDPTCTRL, RXE: 0, RXT: EndpointType::Bulk as u32)
                }
            }
        }
        Ok(())
    }
    fn set_stalled(&mut self, stalled: bool) -> Result<()> {
        let endptctrl = ral::endpoint_control::register(&self.usb, self.address.number().into());

        match self.address.direction() {
            UsbDirection::In => {
                ral::modify_reg!(
                    ral::endpoint_control,
                    &endptctrl,
                    ENDPTCTRL,
                    TXS: stalled as u32
                )
            }
            UsbDirection::Out => {
                ral::modify_reg!(
                    ral::endpoint_control,
                    &endptctrl,
                    ENDPTCTRL,
                    RXS: stalled as u32
                )
            }
        }
        Ok(())
    }
    fn is_stalled(&mut self) -> Result<bool> {
        let endptctrl = ral::endpoint_control::register(&self.usb, self.address.number().into());

        let stalled = match self.address.direction() {
            UsbDirection::In => {
                ral::read_reg!(ral::endpoint_control, &endptctrl, ENDPTCTRL, TXS == 1)
            }
            UsbDirection::Out => {
                ral::read_reg!(ral::endpoint_control, &endptctrl, ENDPTCTRL, RXS == 1)
            }
        };
        Ok(stalled)
    }
}

impl UsbEndpointIn for Endpoint {
    fn write_packet(&mut self, data: &[u8]) -> Result<()> {
        self.check_errors()?;
        if self.is_primed() {
            return Err(UsbError::WouldBlock);
        }
        self.clear_nack();
        let written = self.write(data);
        self.schedule_transfer(written);
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        if self.is_active() {
            Err(UsbError::WouldBlock)
        } else {
            Ok(())
        }
    }
}

impl UsbEndpointOut for Endpoint {
    fn read_packet(&mut self, data: &mut [u8]) -> Result<(usize, OutPacketType)> {
        if self.is_control() && self.has_setup() {
            if data.len() < 8 {
                return Err(UsbError::BufferOverflow);
            }
            let setup = self.read_setup();
            data[..8].copy_from_slice(&setup.to_le_bytes());
            if !self.is_primed() {
                self.clear_nack();
                let max_packet_len = self.max_packet_len();
                self.schedule_transfer(max_packet_len);
            }
            Ok((8, OutPacketType::Setup))
        } else {
            // Caller should only be calling us when we've signaled
            // data via poll()
            self.check_errors()?;
            if self.is_primed() {
                return Err(UsbError::WouldBlock);
            }

            self.clear_complete();
            self.clear_nack();
            let read = self.read(data);
            let max_packet_len = self.max_packet_len();
            self.schedule_transfer(max_packet_len);
            Ok((read, OutPacketType::Data))
        }
    }
}
