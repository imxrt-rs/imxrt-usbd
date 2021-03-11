//! Logging interface, contingent on the hidden `__log` feature
//!
//! Only enable `__log` when debugging, and when you're certain that your
//! logger isn't using USB!

#![allow(unused)]

macro_rules! trace {
    ($($args:tt)*) => {
        #[cfg(feature = "__log")]
        ::log::trace!(target: "", $($args)*)
    };
}

macro_rules! debug {
    ($($args:tt)*) => {
        #[cfg(feature = "__log")]
        ::log::debug!(target: "", $($args)*)
    };
}

macro_rules! info {
    ($($args:tt)*) => {
        #[cfg(feature = "__log")]
        ::log::info!(target: "", $($args)*)
    };
}

macro_rules! warn {
    ($($args:tt)*) => {
        #[cfg(feature = "__log")]
        ::log::warn!(target: "", $($args)*)
    };
}
