//! The example demonstrates how a USB device can reached
//! the 'configured' state. Build the example, and run it
//! on your Teensy 4. You should observe
//!
//! - the LED turns on
//! - a USB device, product string "imxrt-usb," connected to
//!   your system
//!
//! This example also instruments lightweight logging on UART2,
//! pins 14 and 15. Use this example if you need to debug
//! driver initialization.

#![no_std]
#![no_main]

use teensy4_fcb as _;
use teensy4_panic as _;

use hal::ral;
use imxrt_hal as hal;

use pins::common;
use teensy4_pins as pins;

use usb_device::prelude::*;

const UART_BAUD: u32 = 115_200;

#[cortex_m_rt::entry]
fn main() -> ! {
    let hal::Peripherals {
        iomuxc,
        mut ccm,
        dma: _dma,
        uart,
        ..
    } = hal::Peripherals::take().unwrap();
    let pins = pins::t40::into_pins(iomuxc);
    let mut led = configure_led(pins.p13);

    // DMA initialization (for logging)
    let mut dma_channels = _dma.clock(&mut ccm.handle);
    let mut channel = dma_channels[7].take().unwrap();
    channel.set_interrupt_on_completion(false); // We'll poll the logger ourselves...

    //
    // UART initialization (for logging)
    //
    let uarts = uart.clock(
        &mut ccm.handle,
        hal::ccm::uart::ClockSelect::OSC,
        hal::ccm::uart::PrescalarSelect::DIVIDE_1,
    );
    let uart = uarts.uart2.init(pins.p14, pins.p15, UART_BAUD).unwrap();

    let (tx, _) = uart.split();
    imxrt_uart_log::dma::init(tx, channel, Default::default()).unwrap();

    let usb1 = ral::usb::USB1::take().unwrap();
    let phy1 = ral::usbphy::USBPHY1::take().unwrap();
    let mut usb = imxrt_usb::USB::new(usb1, phy1);

    let (ccm, ccm_analog) = ccm.handle.raw();
    ral::modify_reg!(ral::ccm, ccm, CCGR6, CG1: 0b11, CG0: 0b11);

    usb.initialize(ccm_analog);
    set_endpoint_memory(&mut usb);

    let bus_adapter = imxrt_usb::Bus::new(usb);
    let bus = usb_device::bus::UsbBusAllocator::new(bus_adapter);
    let mut device = UsbDeviceBuilder::new(&bus, UsbVidPid(0x5824, 0x27dd))
        .product("imxrt-usb")
        .max_packet_size_0(64)
        .build();

    loop {
        imxrt_uart_log::dma::poll();
        if !device.poll(&mut []) {
            continue;
        }
        let state = device.state();
        if state == usb_device::device::UsbDeviceState::Addressed {
            led.set();
        }
    }
}

type LED = hal::gpio::GPIO<common::P13, hal::gpio::Output>;
fn configure_led(pad: common::P13) -> LED {
    let mut led = hal::gpio::GPIO::new(pad);
    led.set_fast(true);
    led.output()
}

/// Assign memory for all of the USB's endpoints
///
/// # Panics
///
/// Panics if called more than once.
fn set_endpoint_memory(usb: &mut imxrt_usb::USB) {
    use core::sync::atomic;

    static mut ENDPOINT_MEMORY: [u8; 4096] = [0; 4096];
    static ONCE_GUARD: atomic::AtomicBool = atomic::AtomicBool::new(false);

    if ONCE_GUARD.swap(true, atomic::Ordering::SeqCst) {
        panic!("Already allocated endpoint memory!");
    }

    unsafe {
        // Safety: ENDPOINT_MEMORY is unlikely to be null
        let ptr = core::ptr::NonNull::new_unchecked(ENDPOINT_MEMORY.as_mut_ptr());
        // Safety: ENDPOINT_MEMORY valid for it's length. With proper scoping
        // and a runtime flag, we ensure it's only available to a single caller.
        usb.set_endpoint_memory(ptr, ENDPOINT_MEMORY.len());
    }
}
