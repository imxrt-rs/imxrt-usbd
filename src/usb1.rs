//! A USB1 full-speed i.MX RT driver, supporting the `usb-device` ecosystem
//!
//! # Usage
//!
//! 1. Depend on this crate, the `usb-device` crate, a USB class crate that
//!    supports `usb-device`, and [`imxrt-ral`].
//! 2. Create a `BusAdapter` from `imxrt-ral` USB instances.
//! 4. Supply your `BusAdapter` to the `usb-device` devices.
//!
//! See the [`BusAdapter`] documentation for requirements and examples.
//!
//! [`imxrt-ral`]: https://crates.io/crates/imxrt-ral
//! [`usb-device`]: https://crates.io/crates/usb-device

mod bus;
mod driver;
mod endpoint;
mod state;

pub use bus::BusAdapter;
