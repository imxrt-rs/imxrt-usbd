//! A USB1 full-speed i.MX RT driver, supporting the `usb-device` ecosystem
//!
//! # Usage
//!
//! 1. Depend on this crate, the `usb-device` crate, a USB class crate that
//!    supports `usb-device`, and [`imxrt-ral`].
//! 2. Create a `BusAdapter` from `imxrt-ral` USB instances.
//! 4. Use the `BusAdapter` with `usb-device`,
//!
//! See the [`BusAdapter`] documentation for requirements and examples.
//!
//! # Design
//!
//! This section talks about the full-speed driver design. It assumes that
//! you're familiar with the details of the i.MX RT USB peripheral. If you
//! just want to use the driver, you can skip this section.
//!
//! ## Packets and transfers
//!
//! All i.MX RT USB drivers manage queue heads (QH), and transfer
//! descriptors (TD). For the full-speed driver, each (QH) is assigned
//! only one (TD) to perform I/O. We then assume each TD describes a single
//! packet. This is simple to implement, but it means that the full-speed
//! driver can only have one packet in flight per endpoint. You're expected
//! to quickly respond to `poll()` outputs, and schedule the next transfer
//! in the time required for full-speed devices.
//!
//!
//! [`imxrt-ral`]: https://crates.io/crates/imxrt-ral
//! [`usb-device`]: https://crates.io/crates/usb-device

mod bus;
mod driver;
mod endpoint;
mod state;

pub use bus::BusAdapter;
