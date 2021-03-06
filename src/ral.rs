//! imxrt-ral-like API for USB access

pub mod usb;
pub mod usbnc;
pub mod usbphy;

pub use imxrt_ral::{modify_reg, read_reg, write_reg, RORegister, RWRegister};

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

    pub fn register(usb: &super::USB, endpoint: usize) -> EndptCtrl {
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

/// Register addresses for USB1
mod usb1 {
    pub const USB: *const super::usb::RegisterBlock = 0x402e0000 as *const _;
    pub const USBNC: *const super::usbnc::RegisterBlock = 0x402e0000 as *const _;
    pub const USBPHY: *const super::usbphy::RegisterBlock = 0x400d9000 as *const _;

    // Don't panic when you realize that USB and USBNC point to the same address.
    // Given their register block layouts (USBNC has a bunch of reserved memory,
    // right up front), the register offsets work. These were taken from the
    // imxrt-ral code, which is generated from the SVDs. The SVDs are separating
    // USBNC1 from USBNC2, even though the peripheral interleaves the registers.
    // Thanks, NXP.
}

/// Register addresses for USB2
mod usb2 {
    pub const USB: *const super::usb::RegisterBlock = 0x402e0200 as *const _;
    pub const USBNC: *const super::usbnc::RegisterBlock = 0x402e0004 as *const _;
    pub const USBPHY: *const super::usbphy::RegisterBlock = 0x400da000 as *const _;
}

pub struct Instance<RB> {
    pub addr: *const RB,
}

pub type USB = Instance<usb::RegisterBlock>;
pub type USBNC = Instance<usbnc::RegisterBlock>;
pub type USBPHY = Instance<usbphy::RegisterBlock>;

pub enum Inst {
    One,
    Two,
}

pub struct Instances {
    pub usb: USB,
    pub usbnc: USBNC,
    pub usbphy: USBPHY,
}

impl USB {
    /// # Safety
    ///
    /// Allows fabrication of a singleton without taking ownership of the existing
    /// singleton.
    unsafe fn new<P: super::Peripherals>(p: &P) -> Self {
        Self {
            addr: p.core() as *const _,
        }
    }
    pub fn inst(&self) -> Inst {
        match self.addr {
            usb1::USB => Inst::One,
            usb2::USB => Inst::Two,
            _ => unreachable!("USB only constructed after using assert_peripherals"),
        }
    }
}

impl USBNC {
    /// # Safety
    ///
    /// Allows fabrication of a singleton without taking ownership of the existing
    /// singleton.
    unsafe fn new<P: super::Peripherals>(p: &P) -> Self {
        Self {
            addr: p.non_core() as *const _,
        }
    }
}

impl USBPHY {
    /// # Safety
    ///
    /// Allows fabrication of a singleton without taking ownership of the existing
    /// singleton.
    unsafe fn new<P: super::Peripherals>(p: &P) -> Self {
        Self {
            addr: p.phy() as *const _,
        }
    }
}

impl<RB> ::core::ops::Deref for Instance<RB> {
    type Target = RB;
    #[inline(always)]
    fn deref(&self) -> &RB {
        unsafe { &*(self.addr as *const _) }
    }
}

unsafe impl<RB> Send for Instance<RB> {}

/// Assert that the peripheral addresses are valid, and that
/// they correspond to the correct USB instance
///
/// # Panics
///
/// Panics if the pointers are invalid, or if there's a USB1
/// to USB2 mismatch.
fn assert_peripherals<P: super::Peripherals>(p: &P) {
    let usb = p.core() as *const _;
    let usbphy = p.phy() as *const _;
    let usbnc = p.non_core() as *const _;

    if usb == usb1::USB {
        assert_eq!(usbphy, usb1::USBPHY);
        assert_eq!(usbnc, usb1::USBNC);
    } else if usb == usb2::USB {
        assert_eq!(usbphy, usb2::USBPHY);
        assert_eq!(usbnc, usb2::USBNC);
    } else {
        panic!("invalid USB register block pointer");
    }
}

impl Instances {
    /// # Panics
    ///
    /// Panics if the peripherals are mismatched
    pub fn new<P: super::Peripherals>(peripherals: P) -> Self {
        assert_peripherals(&peripherals);

        // Safety: we own the peripherals, and user guarantees
        // that this thing owns the memory. We can turn it
        // into separate singletons.
        unsafe {
            let usb = USB::new(&peripherals);
            let usbnc = USBNC::new(&peripherals);
            let usbphy = USBPHY::new(&peripherals);

            Instances { usb, usbnc, usbphy }
        }
    }
}
