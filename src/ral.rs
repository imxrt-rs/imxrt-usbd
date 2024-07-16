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
