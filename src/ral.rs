//! Re-exporting the imxrt-ral APIS
//!
//! The imxrt-ral APIs have inconsistencies, depending on the
//! version we're using. This module makes the APIs consistent
//! based on the "double-instance" feature.
//!
//! It also adds some enhancements for endpoint control access.

// imxrt-ral/imxrt105x and 106x support...
#[cfg(feature = "double-instance")]
pub use imxrt_ral::usb;
// Otherwise, rename usb1 to usb...
#[cfg(not(feature = "double-instance"))]
pub use imxrt_ral::usb1 as usb;

// Contains USBPHY1, USBPY2..
#[cfg(feature = "double-instance")]
pub use imxrt_ral::usbphy;

// Otherwise, need to make the names ourself,
// because it's inconsistent with the USB core
// registers...
#[cfg(not(feature = "double-instance"))]
pub mod usbphy {
    pub use imxrt_ral::usbphy::USBPHY as USBPHY1;
    pub use imxrt_ral::usbphy::*;
}

pub use imxrt_ral::ccm_analog;
pub use imxrt_ral::{modify_reg, read_reg, write_reg, RWRegister};

/// The RAL API requires us to treat all endpoint control registers as unique.
/// We can make it a little easier with this function, the `EndptCtrl` type,
/// and the helper module.
pub mod endpoint_control {
    use crate::ral;

    #[allow(non_snake_case)]
    pub struct EndptCtrl<'a> {
        pub ENDPTCTRL: &'a ral::RWRegister<u32>,
    }

    #[allow(non_snake_case)]
    pub mod ENDPTCTRL {
        pub use super::ral::usb::ENDPTCTRL1::*;
    }

    pub fn register<'a>(usb: &'a ral::usb::Instance, endpoint: usize) -> EndptCtrl<'a> {
        EndptCtrl {
            ENDPTCTRL: match endpoint {
                0 => &usb.ENDPTCTRL0,
                1 => &usb.ENDPTCTRL1,
                2 => &usb.ENDPTCTRL2,
                3 => &usb.ENDPTCTRL3,
                4 => &usb.ENDPTCTRL4,
                5 => &usb.ENDPTCTRL5,
                6 => &usb.ENDPTCTRL6,
                7 => &usb.ENDPTCTRL7,
                _ => unreachable!("ENDPTCTRL register {} doesn't exist", endpoint),
            },
        }
    }
}
