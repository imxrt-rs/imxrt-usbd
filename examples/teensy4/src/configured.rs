//! The example demonstrates how a USB device can reached
//! the 'configured' state. Build the example, and run it
//! on your Teensy 4. You should observe
//!
//! - the LED turns on
//! - a USB device, product string "imxrt-usbd," connected to
//!   your system
//!
//! This example also instruments lightweight logging on UART2,
//! pins 14 and 15. Use this example if you need to debug
//! driver initialization.

#![no_std]
#![no_main]

use usb_device::prelude::*;

const UART_BAUD: u32 = 115_200;

#[cortex_m_rt::entry]
fn main() -> ! {
    let support::Peripherals {
        mut led, mut ccm, ..
    } = support::setup(core::time::Duration::from_millis(500), UART_BAUD);

    let (ccm, ccm_analog) = ccm.raw();
    support::ccm::initialize(ccm, ccm_analog);

    let bus_adapter = support::new_bus_adapter();

    let bus = usb_device::bus::UsbBusAllocator::new(bus_adapter);
    let mut device = UsbDeviceBuilder::new(&bus, UsbVidPid(0x5824, 0x27dd))
        .product("imxrt-usbd")
        .build();

    loop {
        support::poll_logger();
        if !device.poll(&mut []) {
            continue;
        }
        let state = device.state();
        if state == usb_device::device::UsbDeviceState::Configured {
            led.set();
        }
    }
}
