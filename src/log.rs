//! Logging interface, contingent on the hidden `log` feature
//!
//! Only enable `log` when debugging, and when you're certain that your
//! logger isn't using USB!

macro_rules! trace {
    ($($args:tt)*) => {
        #[cfg(feature = "log")]
        ::log::trace!($($args)*)
    };
}

macro_rules! debug {
    ($($args:tt)*) => {
        #[cfg(feature = "log")]
        ::log::debug!($($args)*)
    };
}

macro_rules! warn {
    ($($args:tt)*) => {
        #[cfg(feature = "log")]
        ::log::warn!($($args)*)
    };
}
