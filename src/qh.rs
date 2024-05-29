//! Endpoint Queue Head (QH)

#![allow(non_snake_case, non_upper_case_globals)]

use crate::ral;
use crate::{td::Td, vcell::VCell};

#[repr(C, align(64))]
pub struct Qh {
    CAPABILITIES: VCell<u32>,
    // No need to see this...
    _current_td_pointer: u32,
    overlay: Td,
    setup: VCell<u64>,
}

impl Qh {
    /// Create a new QH, setting all bits to zero
    pub const fn new() -> Self {
        Qh {
            CAPABILITIES: VCell::new(0),
            _current_td_pointer: 0,
            overlay: Td::new(),
            setup: VCell::new(0),
        }
    }

    /// Read the value from the setup buffer
    ///
    /// Performs a load from the memory dedicated for the setup buffer.
    /// Caller is responsible for managing the setup tripwire, or lockout.
    #[inline(always)]
    pub fn setup(&self) -> u64 {
        self.setup.read()
    }

    /// Returns the next TD overlay
    pub fn overlay_mut(&mut self) -> &mut Td {
        &mut self.overlay
    }

    /// Sets the maximum packet length
    ///
    /// Clamps `max_packet_len` to 1024.
    pub fn set_max_packet_len(&mut self, max_packet_len: usize) {
        ral::modify_reg!(crate::qh, self, CAPABILITIES, MAXIMUM_PACKET_LENGTH: max_packet_len.min(1024) as u32);
    }

    /// Returns the maximum packet length
    pub fn max_packet_len(&self) -> usize {
        ral::read_reg!(crate::qh, self, CAPABILITIES, MAXIMUM_PACKET_LENGTH) as usize
    }

    /// Enable (true) or disable (false) zero length termination
    pub fn set_zero_length_termination(&mut self, zlt: bool) {
        // 0 == Enable zero length packet when transfer is equal to multiple of max packet length
        // 1 == Disable zero length packet
        ral::modify_reg!(crate::qh, self, CAPABILITIES, ZLT: !zlt as u32);
    }

    /// Enable (true) or disable (false) interrupt on setup
    pub fn set_interrupt_on_setup(&mut self, ios: bool) {
        ral::modify_reg!(crate::qh, self, CAPABILITIES, IOS: ios as u32);
    }

    /// Clean and invalidate this QH from DCache
    pub fn clean_invalidate_dcache(&self) {
        crate::cache::clean_invalidate_dcache_by_address(
            &self as *const _ as usize,
            core::mem::size_of_val(self),
        );
    }
}

mod CAPABILITIES {
    pub mod ZLT {
        pub const offset: u32 = 29;
        pub const mask: u32 = 1 << offset;
        pub mod RW {}
        pub mod R {}
        pub mod W {}
    }
    pub mod MAXIMUM_PACKET_LENGTH {
        pub const offset: u32 = 16;
        pub const mask: u32 = 0x7FF << offset;
        pub mod RW {}
        pub mod R {}
        pub mod W {}
    }
    pub mod IOS {
        pub const offset: u32 = 15;
        pub const mask: u32 = 1 << offset;
        pub mod RW {}
        pub mod R {}
        pub mod W {}
    }
}

const _: [(); 1] = [(); (core::mem::size_of::<Qh>() <= 64) as usize];

#[cfg(test)]
mod test {
    use super::Qh;

    #[test]
    fn max_packet_len() {
        let mut qh = Qh::new();
        qh.set_max_packet_len(0x333);
        assert_eq!(qh.max_packet_len(), 0x333);
        assert_eq!(qh.CAPABILITIES.read(), 0x333 << 16);
    }

    #[test]
    fn ios() {
        let mut qh = Qh::new();
        qh.set_interrupt_on_setup(true);
        assert_eq!(qh.CAPABILITIES.read(), 1 << 15);
    }

    #[test]
    fn zlt() {
        let mut qh = Qh::new();
        qh.set_zero_length_termination(false);
        assert_eq!(qh.CAPABILITIES.read(), 1 << 29);
    }
}
