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

pub mod usb1;

/// Eight endpoints, two directions
const QH_COUNT: usize = 8 * 2;
