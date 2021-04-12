//! A USB driver for i.MX RT processors
//!
//! `imxrt-usbd` provides a [`usb-device`] USB bus implementation, allowing you
//! to add USB device features to your embedded Rust program. See each module
//! for usage and examples.
//!
//! To interface the library, you must define a safe implementation of [`Peripherals`].
//! See the trait documentation for more information.
//!
//! # General guidance
//!
//! The driver does not configure any of the CCM or CCM_ANALOG registers. You are
//! responsible for configuring these peripherals for proper USB functionality. See
//! the `imxrt-usbd` hardware examples to see different ways of configuring PLLs and
//! clocks.
//!
//! [`usb-device`]: https://crates.io/crates/usb-device

#![no_std]

#[macro_use]
mod log;

mod buffer;
mod cache;
mod qh;
mod ral;
mod td;
mod vcell;

pub mod full_speed;

/// Eight endpoints, two directions
const QH_COUNT: usize = 8 * 2;

/// A type that owns all USB register blocks
///
/// An implementation of `Peripherals` is expected to own the USB1
/// or USB2 registers. This includes
///
/// - USB core registers
/// - USB PHY registers
/// - USB non-core registers
/// - USB analog registers
///
/// When an instance of `Peripherals` exists, you must make sure that
/// nothing else, other than the owner of the `Peripherals` object,
/// accesses those registers.
///
/// # Safety
///
/// `Peripherals` should only be implemented on a type that
/// owns the various register blocks required for all USB
/// operation. Incorrect usage, or failure to ensure exclusive
/// ownership, could lead to data races and incorrect USB functionality.
///
/// By implementing this trait, you ensure that the [`Instance`]
/// identifier is valid for your chip. Not all i.MX RT peripherals
/// have a USB2 peripheral instance, so you must ensure that the
/// implementation is correct for your chip.
///
/// # Example
///
/// A safe implementation of `Peripherals` that works with the
/// `imxrt-ral` register access layer. Assume that `ral` is
/// shorthand for `imxrt_ral`, like
///
/// ```
/// use imxrt_ral as ral;
/// ```
///
/// ```no_run
/// # mod ral {
/// #   use core::ops::Deref; pub struct Instance; impl Deref for Instance { type Target = u32; fn deref(&self) -> &u32 { unsafe { &*(0x402e0200 as *const u32)} } }
/// #   pub fn take() -> Result<Instance, ()> { Ok(Instance) }
/// #   pub mod usb { pub use super::Instance; pub mod USB1 { pub use super::super::take; } }
/// #   pub mod usbphy { pub use super::Instance; pub mod USBPHY1 { pub use super::super::take; } }
/// #   pub mod usbnc { pub use super::Instance; pub mod USBNC1 { pub use super::super::take; } }
/// #   pub mod usb_analog { pub use super::Instance; pub mod USB_ANALOG { pub use super::super::take; } }
/// # }
/// use ral::usb;
///
/// struct Peripherals {
///     _usb: ral::usb::Instance,
///     _phy: ral::usbphy::Instance,
///     _nc: ral::usbnc::Instance,
///     _analog: ral::usb_analog::Instance,
/// }
///
/// impl Peripherals {
///     /// Panics if the instances are already taken
///     fn usb1() -> Peripherals {
///         Self {
///             _usb: ral::usb::USB1::take().unwrap(),
///             _phy: ral::usbphy::USBPHY1::take().unwrap(),
///             _nc: ral::usbnc::USBNC1::take().unwrap(),
///             _analog: ral::usb_analog::USB_ANALOG::take().unwrap(),
///         }
///     }
/// }
///
/// // This implementation is safe, because a `Peripherals` object
/// // owns the four imxrt-ral instances, which are
/// // guaranteed to be singletons. Given this approach, no one else
/// // can safely access the USB registers.
/// unsafe impl imxrt_usbd::Peripherals for Peripherals {
///     fn instance(&self) -> imxrt_usbd::Instance {
///         imxrt_usbd::Instance::USB1
///     }
/// }
///
/// let peripherals = Peripherals::usb1();
/// let bus = imxrt_usbd::full_speed::BusAdapter::new(
///     peripherals,
///     // Rest of setup...
///     # unsafe { static mut M: [u8; 1] = [0; 1]; &mut M }
/// );
/// ```
pub unsafe trait Peripherals {
    /// Returns the instance identifier for the core registers
    ///
    /// **Warning**: some i.MX RT peripherals have only one USB peripheral,
    /// USB1. The behavior is undefined if you return `Instance::USB2` on
    /// one of these systems.
    fn instance(&self) -> Instance;
}

/// USB instance identifiers
///
/// These are *not* USB standards or speeds. They indicate if this
/// is the USB1 register instance, or the USB2 register instance.
///
/// Note that some i.MX RT processors only have one USB instance (USB1).
/// On those systems, it is invalid to ever indicate the USB2 instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)] // USB naming makes sense here
pub enum Instance {
    /// The first USB register instance
    USB1,
    /// The second USB register instance
    USB2,
}
