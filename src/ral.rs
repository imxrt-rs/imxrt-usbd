//! imxrt-ral-like API for USB access

pub mod usb;

pub use imxrt_ral::{modify_reg, read_reg, write_reg, RORegister, RWRegister};

use crate::CoreRegisters;

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

    pub fn register(usb: &super::usb::Instance, endpoint: usize) -> EndptCtrl {
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

/// # Panics
///
/// Panics if the core registers pointer is invalid.
pub fn instance<C: CoreRegisters>(core_registers: C) -> usb::Instance {
    let ptr = core_registers.as_ptr() as *const _;
    if ptr == usb::USB1 || ptr == usb::USB2 {
        usb::Instance { addr: ptr }
    } else {
        panic!("Incorrect USB core registers address");
    }
}
