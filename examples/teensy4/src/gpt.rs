//! Demonstrates a USB GPT.
//!
//! This example doesn't perform any USB I/O. It simply toggles an LED on a configurable.
//! interval.

#![no_std]
#![no_main]

use hal::ral::interrupt;
use imxrt_usbd::gpt;
use support::hal;
use teensy4_bsp::t40;

//
// Change these constants to test various
// GPT settings.
//

/// How frequently the LED toggles (microseconds).
const LED_PERIOD_US: u32 = 500_000;

//
// The GPT settings below should result in an example
// that appears unchanged. Vary these for quick smoke
// tests.
//

/// The GPT instance we're using.
///
/// This shouldn't matter.
const GPT_INSTANCE: gpt::Instance = gpt::Instance::Gpt1;
/// The GPT mode.
///
/// If set to one-shot, the example will implement the behaviors
/// of repeat mode in software (call `reset()`).
const GPT_MODE: gpt::Mode = gpt::Mode::Repeat;
/// Use an interrupt (`true`) or polling (`false`).
const GPT_INTERRUPT: bool = true;

#[cortex_m_rt::entry]
fn main() -> ! {
    let hal::Peripherals {
        iomuxc, mut ccm, ..
    } = hal::Peripherals::take().unwrap();
    let pins = t40::into_pins(iomuxc);
    let led = teensy4_bsp::configure_led(pins.p13);

    let (ccm, ccm_analog) = ccm.handle.raw();
    support::ccm::initialize(ccm, ccm_analog);

    let bus_adapter = support::new_bus_adapter();
    cortex_m::interrupt::free(|cs| {
        bus_adapter.borrow_gpt(cs, GPT_INSTANCE, |gpt| {
            gpt.stop();
            gpt.clear_elapsed();
            gpt.set_interrupt_enabled(GPT_INTERRUPT);
            gpt.set_mode(GPT_MODE);
            gpt.set_load(LED_PERIOD_US);
            gpt.reset();
        });
    });

    if GPT_INTERRUPT {
        example_interrupt(led, bus_adapter)
    } else {
        example_polling(led, bus_adapter)
    }
}

/// The endless loop when interrupts are enabled.
fn example_interrupt(led: teensy4_bsp::LED, bus_adapter: imxrt_usbd::BusAdapter) -> ! {
    static mut BUS_ADAPTER: Option<imxrt_usbd::BusAdapter> = None;
    static mut LED: Option<teensy4_bsp::LED> = None;

    cortex_m::interrupt::free(|cs| unsafe {
        BUS_ADAPTER = Some(bus_adapter);
        LED = Some(led);
        cortex_m::peripheral::NVIC::unmask(interrupt::USB_OTG1);
        let bus_adapter = BUS_ADAPTER.as_mut().unwrap();
        bus_adapter.borrow_gpt(cs, GPT_INSTANCE, |gpt| gpt.run());
    });

    #[cortex_m_rt::interrupt]
    fn USB_OTG1() {
        cortex_m::interrupt::free(|cs| {
            let bus_adapter = unsafe { BUS_ADAPTER.as_mut().unwrap() };
            let led = unsafe { LED.as_mut().unwrap() };

            bus_adapter.borrow_gpt(cs, GPT_INSTANCE, |gpt| {
                if gpt.is_elapsed() {
                    gpt.clear_elapsed();
                    led.toggle();

                    if GPT_MODE != gpt::Mode::Repeat {
                        gpt.reset();
                    }
                }
            });
        });
    }

    loop {
        cortex_m::asm::wfi()
    }
}

/// The endless loop when interrupts are disabled, and we're polling the
/// GPT timer for completion.
fn example_polling(mut led: teensy4_bsp::LED, bus_adapter: imxrt_usbd::BusAdapter) -> ! {
    // Note: running loop in a critical section. A real
    // system shouldn't do this.
    cortex_m::interrupt::free(|cs| {
        bus_adapter.borrow_gpt(cs, GPT_INSTANCE, |gpt| {
            gpt.run();
            loop {
                if gpt.is_elapsed() {
                    gpt.clear_elapsed();
                    led.toggle();

                    if GPT_MODE != gpt::Mode::Repeat {
                        gpt.reset();
                    }
                }
            }
        })
    })
}
