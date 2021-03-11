//! A full-speed i.MX RT USB driver, supporting the `usb-device` ecosystem
//!
//! # Usage
//!
//! You'll need this crate, the [`usb-device`] crate, and USB device class crates
//! to realize a complete USB device. Use this crate to create a [`BusAdapter`],
//! which is then supplied to the `usb-device` device interface.
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
