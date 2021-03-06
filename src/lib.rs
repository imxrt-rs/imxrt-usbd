//! A USB driver for i.MX RT processors
//!
//! `imxrt-usbd` provides a [`usb-device`] USB bus implementation, allowing you
//! to add USB device features to your embedded Rust program. See each module
//! for usage and examples.
//!
//! To interface the library, you must define a safe implementation of [`Peripherals`].
//! See the peripherals documentation for more information.
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

/// A type that owns USB register blocks
///
/// An implementation of `Peripherals` is expected to own
///
/// - USB core registers
/// - USB non-core registers
/// - USB PHY registers
///
/// For a USB1 or USB2 peripheral.
///
/// # Safety
///
/// `Peripherals` should only be implemented on a type that
/// owns the various register blocks required for all USB
/// operation. The pointer returned by the methods are
/// assumed to be valid, and will be cast to a register
/// definition.
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
/// ```
/// # mod ral {
/// #   use core::ops::Deref; pub struct Instance; impl Deref for Instance { type Target = u32; fn deref(&self) -> &u32 { unsafe { &*(0x402e0200 as *const u32)} } }
/// #   pub fn take() -> Result<Instance, ()> { Ok(Instance) }
/// #   pub mod usb { pub use super::Instance; pub mod USB1 { pub use super::super::take; } }
/// #   pub mod usbnc { pub use super::Instance; pub mod USBNC1 { pub use super::super::take; } }
/// #   pub mod usbphy { pub use super::Instance; pub mod USBPHY1 { pub use super::super::take; } }
/// # }
/// use ral::{usb, usbnc, usbphy};
/// use imxrt_usbd::Peripherals;
///
/// struct Instances {
///     usb: usb::Instance,
///     usbnc: usbnc::Instance,
///     usbphy: usbphy::Instance,
/// }
///
/// impl Instances {
///     /// Panics if the instancs are already taken
///     pub fn usb1() -> Instances {
///         Self {
///             usb: usb::USB1::take().unwrap(),
///             usbnc: usbnc::USBNC1::take().unwrap(),
///             usbphy: usbphy::USBPHY1::take().unwrap(),
///         }
///     }
/// }
///
/// unsafe impl Peripherals for Instances {
///     fn core(&self) -> *const () {
///         &*self.usb as *const _ as _
///     }
///     fn non_core(&self) -> *const () {
///         &*self.usbnc as *const _ as _
///     }
///     fn phy(&self) -> *const () {
///         &*self.usbphy as *const _ as _
///     }
/// }
///
/// let instances = Instances::usb1();
/// assert_eq!(instances.core(), 0x402e0200 as *const ());
/// ```
pub unsafe trait Peripherals {
    /// Returns the address of the USB core registers
    /// for this peripheral instance
    fn core(&self) -> *const ();
    /// Returns the address of the USB non-core registers
    /// for this peripheral instance
    fn non_core(&self) -> *const ();
    /// Returns the address of the USB PHY registers
    /// for this peripheral instance
    fn phy(&self) -> *const ();
}
