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
#![warn(unsafe_op_in_unsafe_fn)]

#[macro_use]
mod log;

mod buffer;
mod bus;
mod cache;
mod driver;
mod endpoint;
mod qh;
mod ral;
mod state;
mod td;
mod vcell;

pub use buffer::EndpointMemory;
pub use bus::{BusAdapter, Speed};
pub mod gpt;
pub use state::{EndpointState, MAX_ENDPOINTS};

/// A type that owns all USB register blocks
///
/// An implementation of `Peripherals` is expected to own the USB1
/// or USB2 registers. This includes
///
/// - USB core registers
/// - USB PHY registers
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
/// All pointers are expected to point at the starting register block
/// for the specified peripheral. Calls to the functions must return the
/// the same value every time they're called.
///
/// # Example
///
/// A safe implementation of `Peripherals` that works with the
/// `imxrt-ral` register access layer.
///
/// ```no_run
/// # mod imxrt_ral {
/// #   pub struct RegisterBlock;
/// #   use core::ops::Deref; pub struct Instance; impl Deref for Instance { type Target = RegisterBlock; fn deref(&self) -> &RegisterBlock { unsafe { &*(0x402e0200 as *const RegisterBlock)} } }
/// #   pub fn take() -> Result<Instance, ()> { Ok(Instance) }
/// #   pub mod usb { pub use super::{Instance, RegisterBlock}; pub mod USB1 { pub use super::super::take; } }
/// #   pub mod usbphy { pub use super::{Instance, RegisterBlock}; pub mod USBPHY1 { pub use super::super::take; } }
/// #   pub mod usbnc { pub use super::Instance; pub mod USBNC1 { pub use super::super::take; } }
/// #   pub mod usb_analog { pub use super::Instance; pub mod USB_ANALOG { pub use super::super::take; } }
/// # }
/// use imxrt_ral as ral;
/// use ral::usb;
///
/// struct Peripherals {
///     usb: ral::usb::Instance,
///     phy: ral::usbphy::Instance,
///     _nc: ral::usbnc::Instance,
///     _analog: ral::usb_analog::Instance,
/// }
///
/// impl Peripherals {
///     /// Panics if the instances are already taken
///     fn usb1() -> Peripherals {
///         Self {
///             usb: ral::usb::USB1::take().unwrap(),
///             phy: ral::usbphy::USBPHY1::take().unwrap(),
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
///     fn usb(&self) -> *const () {
///         let rb: &ral::usb::RegisterBlock = &self.usb;
///         (rb as *const ral::usb::RegisterBlock).cast()
///     }
///     fn usbphy(&self) -> *const () {
///         let rb: &ral::usbphy::RegisterBlock = &self.phy;
///         (rb as *const ral::usbphy::RegisterBlock).cast()
///     }
/// }
///
/// let peripherals = Peripherals::usb1();
/// let bus = imxrt_usbd::BusAdapter::new(
///     peripherals,
///     // Rest of setup...
///     # { static EP_MEMORY: imxrt_usbd::EndpointMemory<1> = imxrt_usbd::EndpointMemory::new(); &EP_MEMORY },
///     # { static EP_STATE: imxrt_usbd::EndpointState = imxrt_usbd::EndpointState::max_endpoints(); &EP_STATE }
/// );
/// ```
pub unsafe trait Peripherals {
    /// Returns the pointer to the USB register block.
    fn usb(&self) -> *const ();
    /// Returns the pointer to the USBPHY register block.
    fn usbphy(&self) -> *const ();
}
