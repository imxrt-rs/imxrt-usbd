//! A USB driver for i.MX RT processors
//!
//! `imxrt-usbd` provides a [`usb-device`] USB bus implementation, allowing you
//! to add USB device features to your embedded Rust program. See each module
//! for usage and examples.
//!
//! # General guidance
//!
//! The driver does not configure any of the CCM or CCM_ANALOG registers. You are
//! responsible for configuring these peripherals for proper USB functionality. See
//! the `imxrt-usbd` hardware examples to see different ways of configuring PLLs and
//! clocks.
//!
//! You, or something in your dependency hierarchy, must enable an `imxrt-ral`
//! chip feature; otherwise, this package will not build.
//!
//! [`usb-device`]: https://crates.io/crates/usb-device
//!
//! # Debugging features
//!
//! Enable the `defmt` feature to activate internal logging using defmt.
//!
//! # Transfer queue (`transfer` feature)
//!
//! Enable the `transfer` feature to unlock a multi-transfer-in-flight path for
//! bulk and interrupt endpoints. The feature adds:
//!
//! - **[`BusAdapter::submit_write`] / [`BusAdapter::submit_read`]** — queue a
//!   zero-copy IN/OUT transfer.  The DMA buffer must remain valid and unmodified
//!   until [`BusAdapter::poll_transfer`] retires it.
//! - **[`BusAdapter::poll_transfer`]** — retire the oldest completed transfer.
//! - **[`BusAdapter::pending_transfers`]** — count in-flight transfers.
//! - **[`BusAdapter::set_packet_queue_depth`]** — configure eager read-ahead
//!   depth for CDC/interrupt OUT endpoints.
//!
//! The per-endpoint TD pool (`TDS_PER_EP = 8`) allows up to eight 16 KiB dTDs
//! per endpoint to be queued simultaneously (128 KiB maximum per transfer chain).
//!
//! ## Lazy vs. eager OUT priming
//!
//! By default (depth 1) OUT endpoints are *lazy*: no staging transfer is primed
//! until the class calls `UsbBus::read`. This prevents the controller from
//! swallowing a packet into a staging buffer while a zero-copy data-phase transfer
//! is active on the same endpoint.
//!
//! Call `set_packet_queue_depth(ep, N)` with N > 1 to switch to *eager* mode:
//! up to N staging transfers are kept primed at all times, suitable for CDC
//! receive paths that want low latency without explicit per-packet priming.
//!
//! # Example
//!
//! ```no_run
//! use imxrt_ral as ral;
//! use imxrt_usbd::{BusAdapter, Instances};
//!
//! static EP_MEMORY: imxrt_usbd::EndpointMemory<1024> = imxrt_usbd::EndpointMemory::new();
//! static EP_STATE: imxrt_usbd::EndpointState = imxrt_usbd::EndpointState::max_endpoints();
//!
//! let instances = Instances {
//!     usb: unsafe { ral::usb::USB::instance() },
//!     usbnc: unsafe { ral::usbnc::USBNC::instance() },
//!     usbphy: unsafe { ral::usbphy::USBPHY::instance() },
//! };
//!
//! let bus_adapter = BusAdapter::new(
//!     instances,
//!     &EP_MEMORY,
//!     &EP_STATE,
//! );
//! ```

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

/// Aggregate of `imxrt-ral` USB peripheral instances.
///
/// This takes ownership of USB peripheral instances for a given USB
/// controller. The const generic `N` ensures that all instances refer
/// to the same USB peripheral (e.g., USB1 or USB2).
pub struct Instances<const N: u8> {
    /// USB core registers.
    pub usb: imxrt_ral::usb::Instance<N>,
    /// USB non-core registers.
    pub usbnc: imxrt_ral::usbnc::Instance<N>,
    /// USBPHY registers.
    pub usbphy: imxrt_ral::usbphy::Instance<N>,
}
