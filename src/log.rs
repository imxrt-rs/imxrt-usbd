//! Logging interface, contingent on the hidden `__log` feature
//!
//! Only enable `__log` when debugging, and when you're certain that your
//! logger isn't using USB!

macro_rules! debug {
    ($($args:tt)*) => {
        #[cfg(feature = "__log")]
        ::__log::debug!($($args)*)
    };
}

macro_rules! warn {
    ($($args:tt)*) => {
        #[cfg(feature = "__log")]
        ::__log::warn!($($args)*)
    };
}
