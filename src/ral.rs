//! Re-exports and helpers for imxrt-ral register access.

pub use imxrt_ral::{modify_reg, read_reg, usb, usbphy, write_reg};

/// The "don't care" peripheral instance number.
const ANY_INSTANCE: u8 = u8::MAX;

/// A USB instance without its compile-time instance number.
pub type AnyUsbInstance = usb::Instance<{ ANY_INSTANCE }>;

/// A USBPHY instance without its compile-time instance number.
pub type AnyUsbphyInstance = usbphy::Instance<{ ANY_INSTANCE }>;

/// Discard the compile-time instance number from an imxrt-ral instance.
///
/// # Safety
///
/// A properly-constructed instance points to static MMIO and is
/// assumed to "own" that MMIO space. We assume static lifetime and
/// take ownership.
fn into_any<T, const N: u8>(
    inst: imxrt_ral::Instance<T, N>,
) -> imxrt_ral::Instance<T, { ANY_INSTANCE }> {
    unsafe {
        let rb: *const T = &*inst;
        imxrt_ral::Instance::new(rb)
    }
}

pub(crate) struct ErasedInstances {
    pub usb: AnyUsbInstance,
    pub usbphy: AnyUsbphyInstance,
}

/// Convert typed imxrt-ral instances into type-erased instances.
///
/// The USBNC instance is consumed but not used by the driver internally.
pub(crate) fn erase_instances<const N: u8>(instances: super::Instances<N>) -> ErasedInstances {
    let super::Instances {
        usb,
        usbnc: _,
        usbphy,
    } = instances;
    ErasedInstances {
        usb: into_any(usb),
        usbphy: into_any(usbphy),
    }
}

/// The RAL API requires us to treat all endpoint control registers as unique.
/// We can make it a little easier with this function, the `EndptCtrl` type,
/// and the helper module.
pub mod endpoint_control {
    use imxrt_ral as ral;

    #[allow(non_snake_case)]
    pub struct EndptCtrl<'a> {
        pub ENDPTCTRL: &'a ral::RWRegister<u32>,
    }

    #[allow(non_snake_case)]
    pub mod ENDPTCTRL {
        pub use imxrt_ral::usb::ENDPTCTRL::*;
    }

    pub fn register(usb: &super::AnyUsbInstance, endpoint: usize) -> EndptCtrl<'_> {
        EndptCtrl {
            ENDPTCTRL: if endpoint == 0 {
                &usb.ENDPTCTRL0
            } else {
                &usb.ENDPTCTRL[endpoint - 1]
            },
        }
    }
}

/// The address of `ptr` in the controller's 32-bit address space.
///
/// The i.MX RT address space is 32 bits wide, so the narrowing is exact on the
/// target. Host unit tests run with wider pointers; they keep the low 32 bits,
/// which is enough to exercise the register layout.
pub(crate) fn bus_address<T>(ptr: *const T) -> u32 {
    ptr.addr() as u32
}

/// The low halfword of a 32-bit register value.
///
/// For the endpoint status registers, whose event bits all sit in the low 16
/// bits (`ENDPTSETUPSTAT` is bits 15-0; `ERCE` and `ETCE` are 8-bit fields
/// within them), so the narrowing drops nothing but reserved bits.
pub(crate) fn low_halfword(value: u32) -> u16 {
    (value & 0xFFFF) as u16
}

#[cfg(test)]
mod test {
    use super::{bus_address, low_halfword};

    #[test]
    fn bus_address_is_the_pointer_value() {
        assert_eq!(bus_address(0x2000_1234 as *const u8), 0x2000_1234);
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn bus_address_keeps_the_low_word_of_a_wide_pointer() {
        assert_eq!(bus_address(0x1_2000_1234usize as *const u8), 0x2000_1234);
    }

    #[test]
    fn low_halfword_keeps_the_low_16_bits() {
        assert_eq!(low_halfword(0xABCD_1234), 0x1234);
        assert_eq!(low_halfword(0x0000_FFFF), 0xFFFF);
        assert_eq!(low_halfword(0x0001_0000), 0);
    }
}
