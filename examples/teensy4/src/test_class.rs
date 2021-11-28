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

const UART_BAUD: u32 = 115_200;
const TESTING_BLINK_PERIOD: core::time::Duration = core::time::Duration::from_millis(200);

#[cortex_m_rt::entry]
fn main() -> ! {
    let support::Peripherals {
        mut led,
        mut gpt1,
        mut ccm,
    } = support::setup(TESTING_BLINK_PERIOD, UART_BAUD);

    let (ccm, ccm_analog) = ccm.raw();
    support::ccm::initialize(ccm, ccm_analog);

    let bus_adapter = support::new_bus_adapter();
    bus_adapter.set_interrupts(true);

    unsafe {
        let bus = usb_device::bus::UsbBusAllocator::new(bus_adapter);
        BUS = Some(bus);
        let bus = BUS.as_ref().unwrap();

        let test_class = usb_device::test_class::TestClass::new(bus);
        CLASS = Some(test_class);
        let test_class = CLASS.as_ref().unwrap();

        let device = test_class.make_device(bus);

        DEVICE = Some(device);

        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        cortex_m::peripheral::NVIC::unmask(interrupt::USB_OTG1);
    }

    gpt1.set_enable(true);
    led.set();
    loop {
        support::poll_logger();
        support::time_elapse(&mut gpt1, || led.toggle());
        cortex_m::asm::wfi();
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
