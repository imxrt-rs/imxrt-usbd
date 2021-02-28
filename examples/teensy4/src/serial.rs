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

use imxrt_hal as hal;
use teensy4_pins as pins;

use usb_device::prelude::*;

const UART_BAUD: u32 = 115_200;
const GPT_OCR: hal::gpt::OutputCompareRegister = hal::gpt::OutputCompareRegister::One;
const BLINK_PERIOD: core::time::Duration = core::time::Duration::from_millis(500);

#[cortex_m_rt::entry]
fn main() -> ! {
    let hal::Peripherals {
        iomuxc,
        mut ccm,
        dma,
        uart,
        mut dcdc,
        gpt1,
        ..
    } = hal::Peripherals::take().unwrap();
    let pins = pins::t40::into_pins(iomuxc);
    let mut led = support::configure_led(pins.p13);

    // Timer for blinking
    let (_, ipg_hz) =
        ccm.pll1
            .set_arm_clock(imxrt_hal::ccm::PLL1::ARM_HZ, &mut ccm.handle, &mut dcdc);

    let mut cfg = ccm.perclk.configure(
        &mut ccm.handle,
        hal::ccm::perclk::PODF::DIVIDE_3,
        hal::ccm::perclk::CLKSEL::IPG(ipg_hz),
    );

    let mut gpt1 = gpt1.clock(&mut cfg);

    gpt1.set_wait_mode_enable(true);
    gpt1.set_mode(imxrt_hal::gpt::Mode::Reset);
    gpt1.set_enable(true);

    gpt1.set_output_compare_duration(GPT_OCR, BLINK_PERIOD);

    // DMA initialization (for logging)
    let mut dma_channels = dma.clock(&mut ccm.handle);
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

    let (ccm, _) = ccm.handle.raw();
    hal::ral::modify_reg!(hal::ral::ccm, ccm, CCGR6, CG1: 0b11, CG0: 0b11);

    let bus_adapter = support::new_bus_adapter();
    let bus = usb_device::bus::UsbBusAllocator::new(bus_adapter);

    let mut serial = usbd_serial::SerialPort::new(&bus);
    let mut device = UsbDeviceBuilder::new(&bus, UsbVidPid(0x5824, 0x27dd))
        .product("imxrt-usbd")
        .max_packet_size_0(64)
        .device_class(usbd_serial::USB_CLASS_CDC)
        .build();

    loop {
        imxrt_uart_log::dma::poll();
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
        time_elapse(&mut gpt1, || led.toggle());
        imxrt_uart_log::dma::poll();
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

fn time_elapse(gpt: &mut hal::gpt::GPT, func: impl FnOnce()) {
    let mut status = gpt.output_compare_status(GPT_OCR);
    if status.is_set() {
        status.clear();
        func();
    }
}
