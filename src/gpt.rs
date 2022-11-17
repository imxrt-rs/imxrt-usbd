//! USB general purpose timers.
//!
//! Each USB OTG peripheral has two general purpose timers (GPT). You can access
//! GPTs through your USB driver.
//!
//! # Example
//!
//! This example shows how to access a GPT through the
//! [`BusAdapter`](crate::BusAdapter) API. The example skips
//! the bus adapter and USB device setup in order to focus on the GPT API. See the bus
//! adapter documentation for more information.
//!
//! ```no_run
//! use imxrt_usbd::BusAdapter;
//! use imxrt_usbd::gpt;
//!
//! # struct Ps;
//! # unsafe impl imxrt_usbd::Peripherals for Ps { fn usb(&self) -> *const () { panic!() } fn usbphy(&self) -> *const () { panic!() } }
//! # static EP_MEMORY: imxrt_usbd::EndpointMemory<1024> = imxrt_usbd::EndpointMemory::new();
//! # static EP_STATE: imxrt_usbd::EndpointState = imxrt_usbd::EndpointState::max_endpoints();
//!
//! # let my_usb_peripherals = // Your Peripherals instance...
//! #   Ps;
//! let bus_adapter = BusAdapter::new(
//!     // ...
//! #    my_usb_peripherals,
//! #    &EP_MEMORY,
//! #    &EP_STATE,
//! );
//!
//! // Prepare a GPT before creating a USB device;
//! bus_adapter.gpt_mut(gpt::Instance::Gpt0, |gpt| {
//!     gpt.stop(); // Stop the timer, just in case it's already running...
//!     gpt.clear_elapsed(); // Clear any outstanding elapsed flags
//!     gpt.set_interrupt_enabled(false); // Enable or disable interrupts
//!     gpt.set_load(75_000); // Elapse after 75ms (75000us)
//!     gpt.set_mode(gpt::Mode::Repeat); // Repeat the timer after it elapses
//!     gpt.reset(); // Load the value into the counter
//! });
//! // The timer isn't running until you call run()...
//!
//! # use usb_device::prelude::*;
//! let bus_allocator = usb_device::bus::UsbBusAllocator::new(bus_adapter);
//!
//! let mut device = UsbDeviceBuilder::new(&bus_allocator, UsbVidPid(0x5824, 0x27dd))
//!     .product("imxrt-usbd")
//!     .build();
//!
//! // You can still access the timer through the bus() method on
//! // the USB device.
//! device.bus().gpt_mut(gpt::Instance::Gpt0, |gpt| gpt.run()); // Timer running!
//!
//! loop {
//!     device.bus().gpt_mut(gpt::Instance::Gpt0, |gpt| {
//!         if gpt.is_elapsed() {
//!             gpt.clear_elapsed();
//!             // Timer elapsed!
//!
//!             // If your mode is Mode::OneShot, you will need
//!             // to call reset() to re-enable the timer. You also
//!             // need to call reset() whenever you change the timer
//!             // load value.
//!         }
//!     });
//! }
//! ```

use crate::ral;

/// GPT timer mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Mode {
    /// In one shot mode, the timer will count down to zero, generate an interrupt,
    /// and stop until the counter is reset by software.
    OneShot = 0,
    /// In repeat mode, the timer will count down to zero, generate an interrupt and
    /// automatically reload the counter value to start again.
    Repeat = 1,
}

/// GPT instance identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Instance {
    /// The GPT0 timer instance.
    Gpt0,
    /// The GPT1 timer instance.
    Gpt1,
}

/// General purpose timer (GPT).
///
/// USB GPTs have a 1us resolution. The counter is 24 bits wide. GPTs can generate
/// USB interrupts that are independent of USB protocol interrupts. This lets you
/// add additional, time-driven logic into your USB ISR and driver state machine.
///
/// See the module-level documentation for an example.
pub struct Gpt<'a> {
    /// Borrow of USB registers from a peripheral
    usb: &'a mut ral::usb::Instance,
    /// GPT instance
    gpt: Instance,
}

impl<'a> Gpt<'a> {
    /// Create a GPT instance over the USB core registers.
    ///
    /// *Why take a mutable reference?* The mutable reference prevents you from
    /// creating two GPTs that alias the same GPT instance.
    ///
    /// *Why not `pub`?* The `ral::usb::Instance` isn't compatible with the
    /// `imxrt_ral::usb::Instance` type, since we're duplicating the RAL module
    /// in our package. Since that type isn't exposed outside of this crate, no
    /// one could create this `Gpt`, anyway.
    pub(crate) fn new(usb: &'a mut ral::usb::Instance, gpt: Instance) -> Self {
        Self { usb, gpt }
    }

    /// Returns the GPT instance identifier.
    pub fn instance(&self) -> Instance {
        self.gpt
    }

    /// Run the GPT timer.
    ///
    /// Run will start counting down the timer. Use `stop()` to cancel a running timer.
    pub fn run(&mut self) {
        match self.gpt {
            Instance::Gpt0 => ral::modify_reg!(ral::usb, self.usb, GPTIMER0CTRL, GPTRUN: 1),
            Instance::Gpt1 => ral::modify_reg!(ral::usb, self.usb, GPTIMER1CTRL, GPTRUN: 1),
        }
    }

    /// Indicates if the timer is running (`true`) or stopped (`false`).
    pub fn is_running(&self) -> bool {
        match self.gpt {
            Instance::Gpt0 => ral::read_reg!(ral::usb, self.usb, GPTIMER0CTRL, GPTRUN == 1),
            Instance::Gpt1 => ral::read_reg!(ral::usb, self.usb, GPTIMER1CTRL, GPTRUN == 1),
        }
    }

    /// Stop the timer.
    pub fn stop(&mut self) {
        match self.gpt {
            Instance::Gpt0 => ral::modify_reg!(ral::usb, self.usb, GPTIMER0CTRL, GPTRUN: 0),
            Instance::Gpt1 => ral::modify_reg!(ral::usb, self.usb, GPTIMER1CTRL, GPTRUN: 0),
        }
    }

    /// Reset the timer.
    ///
    /// `reset` loads the counter value. It does not stop a running counter.
    pub fn reset(&mut self) {
        match self.gpt {
            Instance::Gpt0 => ral::modify_reg!(ral::usb, self.usb, GPTIMER0CTRL, GPTRST: 1),
            Instance::Gpt1 => ral::modify_reg!(ral::usb, self.usb, GPTIMER1CTRL, GPTRST: 1),
        }
    }

    /// Set the timer mode.
    pub fn set_mode(&mut self, mode: Mode) {
        match self.gpt {
            Instance::Gpt0 => {
                ral::modify_reg!(ral::usb, self.usb, GPTIMER0CTRL, GPTMODE: mode as u32)
            }
            Instance::Gpt1 => {
                ral::modify_reg!(ral::usb, self.usb, GPTIMER1CTRL, GPTMODE: mode as u32)
            }
        }
    }

    /// Returns the timer mode.
    pub fn mode(&self) -> Mode {
        let mode: u32 = match self.gpt {
            Instance::Gpt0 => {
                ral::read_reg!(ral::usb, self.usb, GPTIMER0CTRL, GPTMODE)
            }
            Instance::Gpt1 => {
                ral::read_reg!(ral::usb, self.usb, GPTIMER1CTRL, GPTMODE)
            }
        };

        if mode == (Mode::Repeat as u32) {
            Mode::Repeat
        } else if mode == (Mode::OneShot as u32) {
            Mode::OneShot
        } else {
            // All raw mode values handled
            unreachable!()
        }
    }

    /// Set the counter load value.
    ///
    /// `us` is the number of microseconds to count. `us` will saturate at a 24-bit value (0xFFFFFF,
    /// or 16.777215 seconds). A value of `0` will result in a 1us delay.
    ///
    /// Note that the load count value is not loaded until the next call to `reset()` (one shot mode)
    /// or until after the timer elapses (repeat mode).
    pub fn set_load(&mut self, us: u32) {
        let count = us.min(0xFF_FFFF).max(1).saturating_sub(1);
        match self.gpt {
            Instance::Gpt0 => ral::write_reg!(ral::usb, self.usb, GPTIMER0LD, count),
            Instance::Gpt1 => ral::write_reg!(ral::usb, self.usb, GPTIMER1LD, count),
        }
    }

    /// Returns the counter load value.
    pub fn load(&self) -> u32 {
        match self.gpt {
            Instance::Gpt0 => ral::read_reg!(ral::usb, self.usb, GPTIMER0LD),
            Instance::Gpt1 => ral::read_reg!(ral::usb, self.usb, GPTIMER1LD),
        }
    }

    /// Indicates if the timer has elapsed.
    ///
    /// If the timer has elapsed, you should clear the elapsed flag with `clear_elapsed()`.
    pub fn is_elapsed(&self) -> bool {
        match self.gpt {
            Instance::Gpt0 => ral::read_reg!(ral::usb, self.usb, USBSTS, TI0 == 1),
            Instance::Gpt1 => ral::read_reg!(ral::usb, self.usb, USBSTS, TI1 == 1),
        }
    }

    /// Clear the flag that indicates the timer has elapsed.
    pub fn clear_elapsed(&mut self) {
        match self.gpt {
            Instance::Gpt0 => ral::write_reg!(ral::usb, self.usb, USBSTS, TI0: 1),
            Instance::Gpt1 => ral::write_reg!(ral::usb, self.usb, USBSTS, TI1: 1),
        }
    }

    /// Enable or disable interrupt generation when the timer elapses.
    ///
    /// If enabled (`true`), an elapsed GPT will generate an interrupt. This happens regardless of the USB
    /// interrupt enable state.
    pub fn set_interrupt_enabled(&mut self, enable: bool) {
        match self.gpt {
            Instance::Gpt0 => ral::modify_reg!(ral::usb, self.usb, USBINTR, TIE0: enable as u32),
            Instance::Gpt1 => ral::modify_reg!(ral::usb, self.usb, USBINTR, TIE1: enable as u32),
        }
    }

    /// Indicates if interrupt generation is enabled.
    pub fn is_interrupt_enabled(&self) -> bool {
        match self.gpt {
            Instance::Gpt0 => ral::read_reg!(ral::usb, self.usb, USBINTR, TIE0 == 1),
            Instance::Gpt1 => ral::read_reg!(ral::usb, self.usb, USBINTR, TIE1 == 1),
        }
    }
}
