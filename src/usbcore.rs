//! USB Core module for the usb-device endpoint-trait prototype
//!
//! This module provides an implementation of the new usb-device
//! endpoint-trait design. It exists along with the default 0.2
//! usb-device release. We've renamed the unrelease crate to
//! `endpoint-trait` to make the distinction clear.

mod allocator;
mod endpoint;

use endpoint::Endpoint;
