//! Demonstrate a mouse (USB HID).
//!
//! Flash your Teensy 4 with this example. You should observe
//! your mouse slowly inching in one direction every time the
//! LED blinks.
//!
//! This example also supports debug logs over UART2, using pins
//! 14 and 15.

#![no_std]
#![no_main]

use usb_device::prelude::*;
use usbd_hid::descriptor::generator_prelude::*;

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

    let mut hid =
        usbd_hid::hid_class::HIDClass::new(&bus, usbd_hid::descriptor::MouseReport::desc(), 10);
    let mut device = UsbDeviceBuilder::new(&bus, UsbVidPid(0x5824, 0x27dd))
        .product("imxrt-usbd")
        .max_packet_size_0(64)
        .build();

    gpt1.set_enable(true);
    loop {
        support::poll_logger();
        if !device.poll(&mut [&mut hid]) {
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
        support::time_elapse(&mut gpt1, || {
            led.toggle();
            hid.push_input(&usbd_hid::descriptor::MouseReport {
                x: 4,
                y: 4,
                buttons: 0,
                pan: 0,
                wheel: 0,
            })
            .unwrap();
        });
        support::poll_logger();
        if !device.poll(&mut [&mut hid]) {
            continue;
        }
    }
}
