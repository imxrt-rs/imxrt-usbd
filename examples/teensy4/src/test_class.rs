//! usb-device test
//!
//! This example turns the Teensy 4 into a USB device that can be tested
//! from the usb-device host-side test framework. See the usb-device
//! documentation for more information on the test, and see the CONTRIBUTING
//! guide for how to use the test framework.
//!
//! This example also shows how you may use an ISR to poll your USB device.

#![no_std]
#![no_main]

use support::hal;
use teensy4_bsp::t40;

const UART_BAUD: u32 = 115_200;
const GPT_OCR: hal::gpt::OutputCompareRegister = hal::gpt::OutputCompareRegister::One;
const TESTING_BLINK_PERIOD: core::time::Duration = core::time::Duration::from_millis(200);

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
    let pins = t40::into_pins(iomuxc);
    let mut led = support::configure_led(pins.p13);

    // Timer for blinking
    let (_, ipg_hz) = ccm
        .pll1
        .set_arm_clock(hal::ccm::PLL1::ARM_HZ, &mut ccm.handle, &mut dcdc);

    let mut cfg = ccm.perclk.configure(
        &mut ccm.handle,
        hal::ccm::perclk::PODF::DIVIDE_3,
        hal::ccm::perclk::CLKSEL::IPG(ipg_hz),
    );

    let mut gpt1 = gpt1.clock(&mut cfg);

    gpt1.set_wait_mode_enable(true);
    gpt1.set_mode(hal::gpt::Mode::Reset);

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
    bus_adapter.set_interrupts(true);

    unsafe {
        let bus = usb_device::bus::UsbBusAllocator::new(bus_adapter);
        BUS = Some(bus);
        let bus = BUS.as_ref().unwrap();

        let test_class = usb_device::test_class::TestClass::new(bus);
        CLASS = Some(test_class);
        let test_class = CLASS.as_ref().unwrap();

        let device = test_class
            .make_device_builder(bus)
            .max_packet_size_0(64)
            .build();

        DEVICE = Some(device);

        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        cortex_m::peripheral::NVIC::unmask(interrupt::USB_OTG1);
    }

    gpt1.set_enable(true);
    gpt1.set_output_compare_duration(GPT_OCR, TESTING_BLINK_PERIOD);
    led.set();
    loop {
        imxrt_uart_log::dma::poll();
        time_elapse(&mut gpt1, || led.toggle());
        cortex_m::asm::wfi();
    }
}

fn time_elapse(gpt: &mut hal::gpt::GPT, func: impl FnOnce()) {
    let mut status = gpt.output_compare_status(GPT_OCR);
    if status.is_set() {
        status.clear();
        func();
    }
}

type Bus = imxrt_usbd::full_speed::BusAdapter;
type Class = usb_device::test_class::TestClass<'static, Bus>;
static mut CLASS: Option<Class> = None;
static mut BUS: Option<usb_device::bus::UsbBusAllocator<Bus>> = None;
static mut DEVICE: Option<usb_device::device::UsbDevice<'static, Bus>> = None;

use hal::ral::interrupt;

#[cortex_m_rt::interrupt]
fn USB_OTG1() {
    // Must track when the device transitions into / out of configuration,
    // so that we can call configure...
    static mut IS_CONFIGURED: bool = false;

    // Safety: we only unmask the interrupt once all three static mut variables
    // are initialized. We're the only ones to use those variables after they're
    // set.
    let device = unsafe { DEVICE.as_mut().unwrap() };
    let class = unsafe { CLASS.as_mut().unwrap() };

    if device.poll(&mut [class]) {
        if device.state() == usb_device::device::UsbDeviceState::Configured {
            if !*IS_CONFIGURED {
                device.bus().configure();
            }
            *IS_CONFIGURED = true;

            class.poll();
        } else {
            *IS_CONFIGURED = false;
        }
    }
}
