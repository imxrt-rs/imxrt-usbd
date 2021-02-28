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
//! We do this because that's the behaviors expected by the `usb-device`
//! implementation. Specifically, `UsbBus::read()` and `write()` are expected
//! to send *packets*. Furthermore, `UsbBus::poll()` is expected to signal
//! *packet* completion. Although a TD can describe a transfer of *N* packets,
//! we cannot know how many packets to associate with each transfer without
//! implementing our own state machine.
//!
//! This detail reveals why we must increase the max packet size for the
//! control endpoint. Consider a `GET_DESCRIPTOR` device request that expects
//! 18 bytes, and a control endpoint that supports a max packet size of 8 bytes.
//! The USB device will call `write()` three times to send 3 packets. To properly
//! relay the data to the host, we would need to buffer the 18 bytes, then
//! schedule a single TD to send the 3 packets. However, supporting this would
//! complicate the driver, and we might be tricking the USB device into thinking
//! we actually sent the data. Instead, we simplify the driver, and require one packet
//! per TD. This works until you need to send more than 64 bytes to satsify an EP0
//! IN data phase, so we're betting that won't happen.
//!
//! ## Future work
//!
//! This design does not take advangate of the high-speed driver design available
//! in i.MX RT processors. Although the only difference between high- and full-speed
//! is the data bandwidth, we've not tested high-speed I/O with this design, and we will
//! not support it. Therefore, the full-speed driver restricts the potential of the
//! bus. A future high-speed driver should account for these limitations.
//!
//!
//! [`imxrt-ral`]: https://crates.io/crates/imxrt-ral
//! [`usb-device`]: https://crates.io/crates/usb-device

mod bus;
mod driver;
mod endpoint;
mod state;

pub use bus::BusAdapter;
