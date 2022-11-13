//! Support library (qualified as `support`) for all examples.

#![no_std]

pub use bsp::hal;
pub use hal::ral;
pub use teensy4_bsp as bsp;

const SPEED: imxrt_usbd::Speed = {
    #[cfg(feature = "high-speed")]
    {
        imxrt_usbd::Speed::High
    }
    #[cfg(not(feature = "high-speed"))]
    {
        imxrt_usbd::Speed::LowFull
    }
};

/// Allocates a `BusAdapter`
///
/// # Panics
///
/// Panics if any of the `imxrt-ral` USB instances are already
/// taken.
pub fn new_bus_adapter() -> imxrt_usbd::BusAdapter {
    // If we're here, we have exclusive access to ENDPOINT_MEMORY
    static mut ENDPOINT_MEMORY: [u8; 4096] = [0; 4096];

    unsafe {
        // Safety: With proper scoping and checks for singleton access, we ensure the memory is
        // only available to a single caller.
        imxrt_usbd::BusAdapter::with_speed(UsbPeripherals::usb1(), &mut ENDPOINT_MEMORY, SPEED)
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    log::error!("{}", info);
    for _ in 0..10_000 {
        imxrt_uart_log::dma::poll();
    }
    teensy4_panic::sos()
}

//
// Keep in sync with the imxrt_usbd::Peripherals example!
//

struct UsbPeripherals {
    _usb: ral::usb::Instance,
    _phy: ral::usbphy::Instance,
    _nc: ral::usbnc::Instance,
    _analog: ral::usb_analog::Instance,
}

impl UsbPeripherals {
    /// Panics if the instances are already taken
    fn usb1() -> UsbPeripherals {
        Self {
            _usb: ral::usb::USB1::take().unwrap(),
            _phy: ral::usbphy::USBPHY1::take().unwrap(),
            _nc: ral::usbnc::USBNC1::take().unwrap(),
            _analog: ral::usb_analog::USB_ANALOG::take().unwrap(),
        }
    }
}

unsafe impl imxrt_usbd::Peripherals for UsbPeripherals {
    fn instance(&self) -> imxrt_usbd::Instance {
        imxrt_usbd::Instance::USB1
    }
}

pub mod ccm {
    use super::ral;

    /// Initialize CCM clocks for USB1
    pub fn initialize(ccm: &ral::ccm::Instance, ccm_analog: &ral::ccm_analog::Instance) {
        // Enable the PLL...
        loop {
            if ral::read_reg!(ral::ccm_analog, ccm_analog, PLL_USB1, ENABLE == 0) {
                ral::write_reg!(ral::ccm_analog, ccm_analog, PLL_USB1_SET, ENABLE: 1);
                continue;
            }
            if ral::read_reg!(ral::ccm_analog, ccm_analog, PLL_USB1, POWER == 0) {
                ral::write_reg!(ral::ccm_analog, ccm_analog, PLL_USB1_SET, POWER: 1);
                continue;
            }
            if ral::read_reg!(ral::ccm_analog, ccm_analog, PLL_USB1, LOCK == 0) {
                continue;
            }
            if ral::read_reg!(ral::ccm_analog, ccm_analog, PLL_USB1, BYPASS == 1) {
                ral::write_reg!(ral::ccm_analog, ccm_analog, PLL_USB1_CLR, BYPASS: 1);
                continue;
            }
            if ral::read_reg!(ral::ccm_analog, ccm_analog, PLL_USB1, EN_USB_CLKS == 0) {
                ral::write_reg!(ral::ccm_analog, ccm_analog, PLL_USB1_SET, EN_USB_CLKS: 1);
                continue;
            }
            break;
        }

        // Enable the clock gates...
        ral::modify_reg!(ral::ccm, ccm, CCGR6, CG0: 0b11);
    }
}

/// Extra peripherals to support the examples.
pub struct Peripherals {
    /// The LED on pin 13.
    pub led: bsp::LED,
    /// A separate timer.
    ///
    /// The time is **disabled** when it's
    /// returned from `setup()`.
    pub gpt1: hal::gpt::GPT,
    /// The CCM handle for USB clocking.
    pub ccm: hal::ccm::Handle,
}

/// Drive the logging implementation.
pub fn poll_logger() {
    imxrt_uart_log::dma::poll();
}

/// Required for proper function of `time_elapse`.
const GPT_OCR: hal::gpt::OutputCompareRegister = hal::gpt::OutputCompareRegister::One;

/// Set up other system functions that support these examples.
///
/// The `duration` affects the GPT period. The `logging_baud`
/// affects the UART baud.
///
/// See the implementation to understand what peripherals are
/// taken, and how the system is configured. Note that you may
/// be responsible for polling the logging implementation.
///
/// # Panics
///
/// Panics if any of the HAL or BSP peripherals are already taken,
/// or if the logging system fails to initialize.
pub fn setup(duration: core::time::Duration, logging_baud: u32) -> Peripherals {
    let hal::Peripherals {
        iomuxc,
        mut ccm,
        dma,
        uart,
        mut dcdc,
        gpt1,
        ..
    } = hal::Peripherals::take().unwrap();
    let pins = bsp::t40::into_pins(iomuxc);
    let led = bsp::configure_led(pins.p13);

    // Set the ARM core to run at 600MHz. IPG clock runs at 25%
    // of that speed.
    let (_, ipg_hz) = ccm
        .pll1
        .set_arm_clock(hal::ccm::PLL1::ARM_HZ, &mut ccm.handle, &mut dcdc);

    // 150MHz / 3 = 50MHz for PERCLK.
    let mut cfg = ccm.perclk.configure(
        &mut ccm.handle,
        hal::ccm::perclk::PODF::DIVIDE_3,
        hal::ccm::perclk::CLKSEL::IPG(ipg_hz),
    );

    // GPT runs on PERCLK.
    let mut gpt1 = gpt1.clock(&mut cfg);
    gpt1.set_wait_mode_enable(true);
    // Once OCR1 compares, reset the counter.
    gpt1.set_mode(hal::gpt::Mode::Reset);

    gpt1.set_output_compare_duration(GPT_OCR, duration);

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
    let uart = uarts.uart2.init(pins.p14, pins.p15, logging_baud).unwrap();

    let (tx, _) = uart.split();
    imxrt_uart_log::dma::init(tx, channel, Default::default()).unwrap();

    Peripherals {
        led,
        gpt1,
        ccm: ccm.handle,
    }
}

/// Once the GPT has elapsed, invoke `func`.
pub fn time_elapse(gpt: &mut hal::gpt::GPT, func: impl FnOnce()) {
    let mut status = gpt.output_compare_status(GPT_OCR);
    if status.is_set() {
        status.clear();
        func();
    }
}
