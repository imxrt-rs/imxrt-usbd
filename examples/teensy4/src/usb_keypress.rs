//! Demonstrates how to send keystrokes via USB
//! This will spam your keyboard with eldritch slogans until you unplug it or press the reprogram button.

#![no_std]
#![no_main]

use teensy4_panic as _;

use core::str::Chars;
use imxrt_usbd::full_speed::BusAdapter;
use teensy4_bsp::LED;
use usb_device::device::UsbDevice;
use usb_device::prelude::{UsbDeviceBuilder, UsbVidPid};
use usbd_hid::descriptor::KeyboardReport;
use usbd_hid::descriptor::SerializedDescriptor;
use usbd_hid::hid_class::HIDClass;

#[cortex_m_rt::entry]
fn main() -> ! {
    let support::Peripherals {
        mut led,
        mut gpt1,
        mut ccm,
    } = support::setup(core::time::Duration::from_millis(500), 115_200);

    //
    //

    let (ccm, ccm_analog) = ccm.raw();
    support::ccm::initialize(ccm, ccm_analog);

    let bus_adapter = support::new_bus_adapter();
    let bus = usb_device::bus::UsbBusAllocator::new(bus_adapter);

    let mut hid = usbd_hid::hid_class::HIDClass::new(&bus, KeyboardReport::desc(), 10);
    let mut device = UsbDeviceBuilder::new(&bus, UsbVidPid(0x5824, 0x27dd))
        .product("imxrt-usbd")
        .build();

    //

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

    keyboard_mission3(led, &mut hid, &mut device)
}

// https://gist.github.com/MightyPork/6da26e382a7ad91b5496ee55fdc73db2
fn keyboard_mission3(
    mut led: LED,
    hid: &mut HIDClass<BusAdapter>,
    device: &mut UsbDevice<BusAdapter>,
) -> ! {
    let mut msg = CodeSequence::new("Ia! Ia! Cthulhu fhtagn!  ");

    loop {
        let would_block = match hid.maybe_push_input(|| msg.next().unwrap()) {
            None => false,
            Some(Ok(_x)) => false,
            Some(Err(_usb_error)) => true,
        };

        if would_block {
            led.set();
        } else {
            led.clear();
        }

        if !device.poll(&mut [hid]) {
            continue;
        }
    }
}

fn simple_kr(modifier: u8, keycodes: [u8; 6]) -> KeyboardReport {
    KeyboardReport {
        modifier,
        reserved: 0,
        leds: 0,
        keycodes,
    }
}

fn translate_char(ch: char) -> Option<KeyboardReport> {
    match ch {
        'a'..='z' => {
            let code = (ch as u8) - b'a' + 4;
            Some(simple_kr(0, [code, 0, 0, 0, 0, 0]))
        }
        'A'..='Z' => {
            let code = (ch as u8) - b'A' + 4;
            Some(simple_kr(2, [code, 0, 0, 0, 0, 0]))
        }
        '!'..=')' => {
            let code = (ch as u8) - b'!' + 0x1e;
            Some(simple_kr(2, [code, 0, 0, 0, 0, 0]))
        }
        ' ' => Some(simple_kr(0, [0x2c, 0, 0, 0, 0, 0])),
        // lots of stuff is missing, and I'm sure there are keyboard layouts that this is incorrect for.
        _ => None,
    }
}

//

struct CodeSequence<'a> {
    orig: &'a str,
    iter: Chars<'a>,
}

impl<'a> CodeSequence<'a> {
    pub fn new(orig: &'a str) -> CodeSequence<'a> {
        CodeSequence {
            orig,
            iter: orig.chars(),
        }
    }

    pub fn generate(&mut self) -> KeyboardReport {
        loop {
            let ch = self.iter.next();
            match ch {
                None => {
                    self.iter = self.orig.chars();
                }
                Some(ch) => {
                    if let Some(code) = translate_char(ch) {
                        return code;
                    }
                }
            }
        }
    }
}

impl<'a> Iterator for CodeSequence<'a> {
    type Item = KeyboardReport;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.generate())
    }
}

// cargo build --release  --target thumbv7em-none-eabihf  --example usb_keypress &&
// cargo objcopy --release  --target thumbv7em-none-eabihf  --bin usb_keypress -- -O ihex /tmp/kbd.hex
