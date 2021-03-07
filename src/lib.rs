//! A USB driver for i.MX RT processors
//!
//! `imxrt-usbd` provides a [`usb-device`] USB bus implementation, allowing you
//! to add USB device features to your embedded Rust program. See each module
//! for usage and examples.
//!
//! To interface the library, you must define a safe implementation of [`CoreRegisters`].
//! See the trait documentation for more information.
//!
//! # General guidance
//!
//! The USB driver takes ownership of the USB core registers. The driver does not configure
//! any of
//!
//! - USBPHY, the integrated PHY registers
//! - USBNC, the non-core registers
//! - USB_ANALOG, the USB analog registers
//!
//! nor does it affect any of the CCM (or CCM_ANALOG) registers. You're responsible for
//! configuring these peripherals for proper USB functionality. See the `imxrt-usbd`
//! hardware examples to see different ways of configuring these registers.
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

/// A type that owns the USB core registers block
///
/// An implementation of `CoreRegisters` is expected to own the USB1
/// or USB2 core registers block. Given this object's ownership of
/// the static memory, it should be unavailable to anyone else in
/// the program.
///
/// # Safety
///
/// `CoreRegisters` should only be implemented on a type that
/// owns the various register blocks required for all USB
/// operation. The returned pointer will be checked for validity
/// before usage.
///
/// # Example
///
/// A safe implementation of `CoreRegisters` that works with the
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
/// # }
/// use ral::usb;
/// use imxrt_usbd::CoreRegisters;
///
/// struct Instances {
///     usb: usb::Instance,
/// }
///
/// impl Instances {
///     /// Panics if the instance is already taken
///     pub fn usb1() -> Instances {
///         Self {
///             usb: usb::USB1::take().unwrap(),
///         }
///     }
/// }
///
/// // Safety: the safe imxrt-ral API ensures that there is only one instance
/// // in any given Rust program. Since we own it, it's safe to implement
/// // CoreRegisters.
/// unsafe impl CoreRegisters for Instances {
///     fn as_ptr(&self) -> *const () {
///         &*self.usb as *const _ as *const ()
///     }
/// }
///
/// let instances = Instances::usb1();
/// assert_eq!(instances.as_ptr(), 0x402e0200 as *const ());
/// ```
pub unsafe trait CoreRegisters {
    /// Returns the address of the USB core registers
    /// for this peripheral instance
    fn as_ptr(&self) -> *const ();
}
