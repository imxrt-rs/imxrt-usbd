//! Optional logging.

#![allow(unused)]

macro_rules! trace {
    ($($args:tt)*) => {
        #[cfg(feature = "defmt")]
        {
            defmt::trace!($($args)*)
        }
    };
}

macro_rules! debug {
    ($($args:tt)*) => {
        #[cfg(feature = "defmt")]
        {
            defmt::debug!($($args)*)
        }
    };
}

macro_rules! info {
    ($($args:tt)*) => {
        #[cfg(feature = "defmt")]
        {
            defmt::info!($($args)*)
        }
    };
}

macro_rules! warn {
    ($($args:tt)*) => {
        #[cfg(feature = "defmt")]
        {
            defmt::warn!($($args)*)
        }
    };
}
