//! Endpoint Transfer Descriptors (TD)
//!
//! The module implements a RAL-compatible interface for working
//! with transfer descriptors.

#![allow(non_snake_case, non_upper_case_globals)]

use crate::vcell::VCell;

#[repr(C)]
pub struct TD {
    pub NEXT: VCell<u32>,
    pub TOKEN: VCell<u32>,
    pub BUFFERS: [VCell<u32>; 5],
    // Reserved memory could be used for other things!
    _reserved: [u32; 1],
}

impl TD {
    pub const fn new() -> Self {
        TD {
            NEXT: VCell::new(0),
            TOKEN: VCell::new(0),
            BUFFERS: [
                VCell::new(0),
                VCell::new(0),
                VCell::new(0),
                VCell::new(0),
                VCell::new(0),
            ],
            _reserved: [0; 1],
        }
    }
}

pub mod NEXT {
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

pub mod TOKEN {
    pub mod STATUS {
        pub const offset: u32 = 0;
        pub const mask: u32 = 0xFF << offset;
        pub mod RW {}
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
    use super::TD;
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
}

const _: [(); 1] = [(); (core::mem::size_of::<TD>() == 32) as usize];
