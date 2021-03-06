//! A USB driver for i.MX RT processors
//!
//! `imxrt-usbd` provides a [`usb-device`] USB bus implementation, allowing you
//! to add USB device features to your embedded Rust program. The package
//! supports all of the i.MX RT chips available in the [`imxrt-ral`] register
//! access layer.
//!
//! # Build
//!
//! `imxrt-usbd` will not build in isolation. It requires that an [`imxrt-ral`]
//! chip-specific feature is enabled in your dependency chain. If that `imxrt-ral`
//! feature is *any* of the following features,
//!
//! - `"imxrt1051"`
//! - `"imxrt1052"`
//! - `"imxrt1061"`
//! - `"imxrt1062"`
//!
//! then you **must** enable this crate's `"double-instance"` feature to properly
//! support the two available USB instances. Failure to specify features will
//! result in a failed build.
//!
//! [`imxrt-ral`]: https://crates.io/crates/imxrt-ral
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

/// A type that owns all USB register blocks, including
///
/// - USB core registers
/// - USB non-core registers
/// - USB PHY registers
/// - USB analog registers
///
/// # Safety
///
/// `Peripherals` should only be implemented on a type that
/// owns the various register blocks required for all USB
/// operation. The pointer returned by the four methods
/// is assumed to be valid, and will be cast to a register
/// definition.
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
    /// Returns the address of the USB analog registers
    /// for this peripheral instance
    fn analog(&self) -> *const ();
}
