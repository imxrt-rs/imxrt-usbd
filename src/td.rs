//! Endpoint Transfer Descriptors (TD)
//!
//! The module implements a RAL-compatible interface for working
//! with transfer descriptors.

#![allow(non_snake_case, non_upper_case_globals)]

use crate::{ral, vcell::VCell};

#[repr(C)]
pub struct TD {
    NEXT: VCell<u32>,
    TOKEN: VCell<u32>,
    BUFFER_POINTERS: [VCell<u32>; 5],
    // Reserved memory could be used for other things!
    _reserved: [u32; 1],
}

impl TD {
    pub const fn new() -> Self {
        TD {
            NEXT: VCell::new(0),
            TOKEN: VCell::new(0),
            BUFFER_POINTERS: [
                VCell::new(0),
                VCell::new(0),
                VCell::new(0),
                VCell::new(0),
                VCell::new(0),
            ],
            _reserved: [0; 1],
        }
    }

    /// Prepare a transfer to / from the memory described by `ptr` and `size`
    ///
    /// Direction is dependent on the endpoint characteristics.
    pub fn set_buffer(&self, ptr: *mut u8, size: usize) {
        ral::write_reg!(crate::td, self, NEXT, TERMINATE: 1);
        ral::write_reg!(
            crate::td,
            self,
            TOKEN,
            TOTAL_BYTES: size as u32,
            STATUS: ACTIVE
        );

        if size != 0 {
            const PTR_ALIGNMENT: u32 = 4096;
            const PTR_MASK: u32 = !(PTR_ALIGNMENT - 1);

            self.BUFFER_POINTERS[0].write(ptr as u32);
            for idx in 1..self.BUFFER_POINTERS.len() {
                let mut ptr = self.BUFFER_POINTERS[idx - 1].read();
                ptr &= PTR_MASK;
                ptr += PTR_ALIGNMENT;
                self.BUFFER_POINTERS[idx].write(ptr);
            }
        } else {
            for buffer_pointer in self.BUFFER_POINTERS.iter() {
                buffer_pointer.write(0);
            }
        }
    }

    /// Read the status of the current / previous transfer
    pub fn status(&self) -> Status {
        let status = ral::read_reg!(crate::td, self, TOKEN, STATUS);
        Status::from_bits_truncate(status)
    }
}

bitflags::bitflags! {
    pub struct Status : u32 {
        const ACTIVE = TOKEN::STATUS::RW::ACTIVE;
        const HALTED = TOKEN::STATUS::RW::HALTED;
        const DATA_BUS_ERROR = TOKEN::STATUS::RW::DATA_BUS_ERROR;
        const TRANSACTION_ERROR = TOKEN::STATUS::RW::TRANSACTION_ERROR;
    }
}

mod NEXT {
    pub mod TERMINATE {
        pub const offset: u32 = 0;
        pub const mask: u32 = 1 << offset;
        pub mod RW {}
        pub mod R {}
        pub mod W {}
    }
    pub mod NEXT_LINK_POINTER {
        pub const offset: u32 = 5;
        pub const mask: u32 = 0x7ffffff << offset;
        pub mod RW {}
        pub mod R {}
        pub mod W {}
    }
}

mod TOKEN {
    pub mod STATUS {
        pub const offset: u32 = 0;
        pub const mask: u32 = 0xFF << offset;
        pub mod RW {
            pub const ACTIVE: u32 = 1 << 7;
            pub const HALTED: u32 = 1 << 6;
            pub const DATA_BUS_ERROR: u32 = 1 << 5;
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
mod test {
    use super::{Status, TD};
    use crate::ral;

    #[test]
    fn next() {
        let td = TD::new();
        ral::write_reg!(super, &td, NEXT, NEXT_LINK_POINTER: u32::max_value());
        assert_eq!(td.NEXT.read(), u32::max_value() & !0b11111);
    }

    #[test]
    fn terminate() {
        let td = TD::new();
        ral::write_reg!(super, &td, NEXT, TERMINATE: u32::max_value());
        assert_eq!(td.NEXT.read(), 1);
    }

    #[test]
    fn status() {
        let td = TD::new();
        ral::write_reg!(super, &td, TOKEN, STATUS: u32::max_value());
        assert_eq!(td.TOKEN.read(), 0b11111111);
    }

    #[test]
    fn ioc() {
        let td = TD::new();
        ral::write_reg!(super, &td, TOKEN, IOC: u32::max_value());
        assert_eq!(td.TOKEN.read(), 1 << 15);
    }

    #[test]
    fn total_bytes() {
        let td = TD::new();
        ral::write_reg!(super, &td, TOKEN, TOTAL_BYTES: u32::max_value());
        assert_eq!(td.TOKEN.read(), 0x7FFF << 16);
    }

    #[test]
    fn set_buffer() {
        let td = TD::new();
        let mut buffer = [0; 32];
        td.set_buffer(buffer.as_mut_ptr(), buffer.len());
        assert_eq!(td.NEXT.read(), 1);
        assert_eq!(td.TOKEN.read(), (32 << 16) | (1 << 7));
        assert!(td.status().contains(Status::ACTIVE));
        for buffer_pointer in td.BUFFER_POINTERS.iter() {
            assert!(buffer_pointer.read() != 0);
        }
    }
}

const _: [(); 1] = [(); (core::mem::size_of::<TD>() == 32) as usize];
