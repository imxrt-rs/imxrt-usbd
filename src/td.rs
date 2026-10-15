//! Endpoint Transfer Descriptors (TD)
//!
//! The module implements a RAL-compatible interface for working
//! with transfer descriptors.

#![allow(non_snake_case, non_upper_case_globals)]

use crate::{ral, vcell::VCell};

/// The page size of a dTD buffer pointer: each pointer selects a 4 KiB page.
pub(crate) const PAGE_SIZE: u32 = 4096;

/// The number of buffer page pointers in a dTD.
pub(crate) const BUFFER_POINTER_COUNT: usize = 5;

#[repr(C)]
pub struct Td {
    NEXT: VCell<u32>,
    TOKEN: VCell<u32>,
    BUFFER_POINTERS: [VCell<u32>; BUFFER_POINTER_COUNT],
    // Reserved memory for other information
    last_transfer_size: usize,
}

impl Td {
    pub const fn new() -> Self {
        Td {
            NEXT: VCell::new(0),
            TOKEN: VCell::new(0),
            BUFFER_POINTERS: [const { VCell::new(0) }; BUFFER_POINTER_COUNT],
            last_transfer_size: 0,
        }
    }

    /// Prepare a transfer to / from the memory described by `ptr` and `size`
    ///
    /// Specifieds `size` as the total bytes expected to transfer. This may not
    /// be what's fully transferred; check `bytes_transferred` after the transfer
    /// completes.
    ///
    /// The transfer must fit the dTD's page pointers: the offset of `ptr` into its
    /// page plus `size` may not exceed `BUFFER_POINTER_COUNT` pages. A debug build
    /// asserts it.
    pub fn set_buffer(&mut self, ptr: *mut u8, size: usize) {
        debug_assert!(
            size == 0
                || (ptr as usize % PAGE_SIZE as usize) + size
                    <= BUFFER_POINTER_COUNT * PAGE_SIZE as usize,
            "transfer of {size} bytes at {ptr:p} runs past the dTD's buffer pages"
        );
        ral::modify_reg!(crate::td, self, TOKEN, TOTAL_BYTES: size as u32);
        self.last_transfer_size = size;

        if size != 0 {
            const PTR_ALIGNMENT: u32 = PAGE_SIZE;
            const PTR_MASK: u32 = !(PTR_ALIGNMENT - 1);

            self.BUFFER_POINTERS[0].write(ptr as u32);
            for idx in 1..self.BUFFER_POINTERS.len() {
                let mut ptr = self.BUFFER_POINTERS[idx - 1].read();
                ptr &= PTR_MASK;
                ptr += PTR_ALIGNMENT;
                self.BUFFER_POINTERS[idx].write(ptr);
            }
        } else {
            for buffer_pointer in self.BUFFER_POINTERS.iter_mut() {
                buffer_pointer.write(0);
            }
        }
    }

    /// Returns the number of bytes transferred in the previous transfer
    pub fn bytes_transferred(&self) -> usize {
        let total_bytes = ral::read_reg!(crate::td, self, TOKEN, TOTAL_BYTES) as usize;
        self.last_transfer_size - total_bytes
    }

    /// Read the status of the current / previous transfer
    pub fn status(&self) -> Status {
        let status = ral::read_reg!(crate::td, self, TOKEN, STATUS);
        Status::from_bits_truncate(status)
    }

    /// Clear all status flags in this transfer descriptor
    pub fn clear_status(&mut self) {
        ral::modify_reg!(crate::td, self, TOKEN, STATUS: 0);
    }

    /// Set the terminate bit to indicate that this TD points to an invalid
    /// next TD
    pub fn set_terminate(&mut self) {
        ral::write_reg!(crate::td, self, NEXT, 1);
    }

    /// Set the next TD pointed at by this TD
    pub fn set_next(&mut self, next: *const Td) {
        ral::write_reg!(crate::td, self, NEXT, next as u32);
    }

    /// Set the active flag
    pub fn set_active(&mut self) {
        ral::modify_reg!(crate::td, self, TOKEN, STATUS: ACTIVE);
    }

    /// Specify if transfer completion should be indicated as a
    /// USB interrupt (irrespective of an actual ISR run)
    pub fn set_interrupt_on_complete(&mut self, ioc: bool) {
        ral::modify_reg!(crate::td, self, TOKEN, IOC: ioc as u32);
    }

    /// Clean and invalidate this TD from DCache
    pub fn clean_invalidate_dcache(&self) {
        crate::cache::clean_invalidate_dcache_of(self);
    }
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Status : u32 {
        const ACTIVE = TOKEN::STATUS::RW::ACTIVE;
        const HALTED = TOKEN::STATUS::RW::HALTED;
        const DATA_BUFFER_ERROR = TOKEN::STATUS::RW::DATA_BUFFER_ERROR;
        const TRANSACTION_ERROR = TOKEN::STATUS::RW::TRANSACTION_ERROR;
    }
}

impl Status {
    /// Returns `true` if any of the error bits is set: transaction error,
    /// data buffer error, or halted.
    pub(crate) fn has_error(self) -> bool {
        self.intersects(Self::TRANSACTION_ERROR | Self::DATA_BUFFER_ERROR | Self::HALTED)
    }
}

/// The dTD token word, as a RAL-style register description.
///
/// The bit positions follow the "dTD Token" table of the i.MX RT1060 Reference
/// Manual (Rev. 1, "Endpoint Transfer Descriptor (dTD)", section 41.5.5.2,
/// Table 41-63):
///
/// | Bits  | Field                         |
/// | ----- | ----------------------------- |
/// | 30-16 | Total Bytes                   |
/// | 15    | Interrupt On Complete (IOC)   |
/// | 11-10 | Multiplier Override (MultO)   |
/// | 7-0   | Status                        |
///
/// MultO must be zero for anything but transmit isochronous endpoints; the driver
/// never sets it. Status bit 7 is Active, bit 6 Halted, bit 5 Data Buffer Error
/// and bit 3 Transaction Error.
mod TOKEN {
    pub mod STATUS {
        pub const offset: u32 = 0;
        pub const mask: u32 = 0xFF << offset;
        pub mod RW {
            pub const ACTIVE: u32 = 1 << 7;
            pub const HALTED: u32 = 1 << 6;
            pub const DATA_BUFFER_ERROR: u32 = 1 << 5;
            pub const TRANSACTION_ERROR: u32 = 1 << 3;
        }
        pub mod R {}
        pub mod W {}
    }
    pub mod IOC {
        pub const offset: u32 = 15;
        pub const mask: u32 = 1 << offset;
        pub mod RW {}
        pub mod R {}
        pub mod W {}
    }
    pub mod TOTAL_BYTES {
        pub const offset: u32 = 16;
        pub const mask: u32 = 0x7FFF << offset;
        pub mod RW {}
        pub mod R {}
        pub mod W {}
    }
}

#[cfg(test)]
impl Td {
    /// Simulate hardware transfer completion.
    ///
    /// Clears `ACTIVE` and sets `TOTAL_BYTES` to `remaining` (residual bytes
    /// not transferred, as the hardware would write). For example, a 512-byte
    /// transfer completed in full has `remaining = 0`; one that delivered only
    /// 31 bytes out of 512 has `remaining = 512 - 31 = 481`.
    ///
    /// This is purely a test helper — on real hardware the controller writes
    /// the TOKEN word directly; we replicate that here via RAL primitives.
    // Used only in `#[cfg(feature = "transfer")]` tests; dead without the feature.
    #[allow(dead_code)]
    pub fn force_complete(&mut self, remaining: usize) {
        // Clear ACTIVE bit.
        ral::modify_reg!(crate::td, self, TOKEN, STATUS: 0);
        // Write TOTAL_BYTES residual — keep other TOKEN fields (IOC, etc.) intact.
        ral::modify_reg!(crate::td, self, TOKEN, TOTAL_BYTES: remaining as u32);
    }
}

#[cfg(test)]
mod test {
    use super::Td;
    use crate::ral;

    #[test]
    fn terminate() {
        let mut td = Td::new();
        td.set_terminate();
        assert_eq!(td.NEXT.read(), 1);
    }

    #[test]
    fn next() {
        let mut td = Td::new();
        td.set_terminate();

        let other = !31u32;
        td.set_next(other as *const _);
        assert_eq!(td.NEXT.read(), other);
    }

    #[test]
    fn status() {
        let mut td = Td::new();
        ral::write_reg!(super, &mut td, TOKEN, STATUS: u32::MAX);
        assert_eq!(td.TOKEN.read(), 0b11111111);
    }

    #[test]
    fn status_has_error() {
        use super::Status;

        for error in [
            Status::TRANSACTION_ERROR,
            Status::DATA_BUFFER_ERROR,
            Status::HALTED,
        ] {
            assert!(error.has_error(), "{error:?} is an error");
            assert!(
                (error | Status::ACTIVE).has_error(),
                "{error:?} is an error while active"
            );
        }
        assert!(!Status::empty().has_error(), "an idle transfer is no error");
        assert!(
            !Status::ACTIVE.has_error(),
            "an active transfer is no error"
        );
    }

    #[test]
    fn ioc() {
        let mut td = Td::new();
        ral::write_reg!(super, &mut td, TOKEN, IOC: u32::MAX);
        assert_eq!(td.TOKEN.read(), 1 << 15);
    }

    #[test]
    fn total_bytes() {
        let mut td = Td::new();
        ral::write_reg!(super, &mut td, TOKEN, TOTAL_BYTES: u32::MAX);
        assert_eq!(td.TOKEN.read(), 0x7FFF << 16);
    }

    #[test]
    fn set_buffer_fills_every_page_pointer() {
        let mut td = Td::new();
        // The pointer is never dereferenced.
        let ptr = 0x2000_0000 as *mut u8;
        td.set_buffer(ptr, 5 * 4096);
        for (idx, buffer_pointer) in td.BUFFER_POINTERS.iter().enumerate() {
            assert_eq!(buffer_pointer.read(), 0x2000_0000 + (idx as u32) * 4096);
        }
    }

    #[test]
    fn set_buffer_accepts_the_largest_payload_at_any_offset() {
        let mut td = Td::new();
        // The last byte of the payload lands on the last byte of the fifth page.
        td.set_buffer(0x2000_0001 as *mut u8, 5 * 4096 - 1);
        // 16 KiB from the last byte of a page spans exactly five pages.
        td.set_buffer(0x2000_0fff as *mut u8, 16 * 1024);
    }

    #[test]
    #[should_panic(expected = "runs past the dTD's buffer pages")]
    fn set_buffer_rejects_a_payload_past_the_pages() {
        let mut td = Td::new();
        td.set_buffer(0x2000_0001 as *mut u8, 5 * 4096);
    }

    #[test]
    fn set_buffer() {
        let mut td = Td::new();
        let mut buffer = [0; 32];
        td.set_buffer(buffer.as_mut_ptr(), buffer.len());
        assert_eq!(td.NEXT.read(), 0);
        assert_eq!(td.TOKEN.read(), (32 << 16));
        assert!(td.status().is_empty());
        for buffer_pointer in td.BUFFER_POINTERS.iter() {
            assert!(buffer_pointer.read() != 0);
        }
    }
}

#[cfg(target_arch = "arm")]
const _: [(); 1] = [(); (core::mem::size_of::<Td>() == 32) as usize];
