//! imxrt-ral-like API for USB access

pub mod usb;
pub mod usbphy;

pub use imxrt_ral::{modify_reg, read_reg, write_reg, RORegister, RWRegister};

use crate::{Instance, Peripherals};

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

    // TODO(mciantyre) usize -> u8 to match with EndpointAddress::number()
    pub fn register<I>(usb: &I, endpoint: usize) -> EndptCtrl
    where
        I: core::ops::Deref<Target = super::usb::RegisterBlock>,
    {
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

pub struct Instances {
    pub usb: usb::Instance,
    pub usbphy: usbphy::Instance,
}

/// Converts the core registers into a USB register block instance
pub fn instances<P: Peripherals>(peripherals: P) -> Instances {
    let usb = usb::Instance {
        addr: match peripherals.instance() {
            Instance::USB1 => usb::USB1,
            Instance::USB2 => usb::USB2,
        },
    };
    let usbphy = usbphy::Instance {
        addr: match peripherals.instance() {
            Instance::USB1 => usbphy::USBPHY1,
            Instance::USB2 => usbphy::USBPHY2,
        },
    };
    Instances { usb, usbphy }
}
