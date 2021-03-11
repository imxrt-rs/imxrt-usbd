//! Demonstrate a USB serial device
//!
//! Flash your Teensy 4 with this example. Then, connect a serial
//! interface to the USB device. You should see all inputs echoed
//! back to you.
//!
//! This example also supports debug logs over UART2, using pins
//! 14 and 15.

#![no_std]
#![no_main]

use usb_device::prelude::*;

const UART_BAUD: u32 = 115_200;
const BLINK_PERIOD: core::time::Duration = core::time::Duration::from_millis(500);

#[cortex_m_rt::entry]
fn main() -> ! {
    let support::Peripherals {
        mut led,
        mut gpt1,
        mut ccm,
    } = support::setup(BLINK_PERIOD, UART_BAUD);

    let (ccm, ccm_analog) = ccm.raw();
    support::ccm::initialize(ccm, ccm_analog);

    let bus_adapter = support::new_bus_adapter();
    let bus = usb_device::bus::UsbBusAllocator::new(bus_adapter);

    let mut serial = usbd_serial::SerialPort::new(&bus);
    let mut device = UsbDeviceBuilder::new(&bus, UsbVidPid(0x5824, 0x27dd))
        .product("imxrt-usbd")
        .device_class(usbd_serial::USB_CLASS_CDC)
        .max_packet_size_0(64)
        .build();

    gpt1.set_enable(true);
    loop {
        support::poll_logger();
        if !device.poll(&mut [&mut serial]) {
            continue;
        }
        let state = device.state();
        if state == usb_device::device::UsbDeviceState::Configured {
            break;
        }
    }

    device.bus().configure();
    led.set();

    loop {
        support::time_elapse(&mut gpt1, || led.toggle());
        support::poll_logger();
        if !device.poll(&mut [&mut serial]) {
            continue;
        }

        let mut buf = [0u8; 64];

        match serial.read(&mut buf[..]) {
            Ok(count) => {
                let s = core::str::from_utf8(&buf).unwrap();
                log::info!("{}", s);
                serial.write(&buf[..count]).ok();
            }
            Err(UsbError::WouldBlock) => log::warn!("WOULDBLOCK"),
            Err(_err) => panic!(),
        };
    }
}
