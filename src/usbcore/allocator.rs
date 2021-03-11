//! USB endpoint allocator and static memory

use crate::QH_COUNT;
use crate::{qh, ral, td};

use endpoint_trait::{
    endpoint::{EndpointAddress, EndpointConfig},
    usbcore::{UsbCore, UsbEndpointAllocator},
    Result, UsbDirection, UsbError,
};

/// A list of transfer descriptors
///
/// Supports 1 TD per QH (per endpoint direction)
#[repr(align(32))]
struct TDList([td::TD; QH_COUNT]);
const TD_LIST_INIT: TDList = TDList([
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
    td::TD::new(),
]);

/// A list of queue heads
///
/// One queue head per endpoint, per direction (default).
#[repr(align(4096))]
struct QHList([qh::QH; QH_COUNT]);
const QH_LIST_INIT: QHList = QHList([
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
    qh::QH::new(),
]);

struct State {
    qhs: QHList,
    tds: TDList,
}

const STATE_INIT: State = State {
    qhs: QH_LIST_INIT,
    tds: TD_LIST_INIT,
};

static mut USB1_STATE: State = STATE_INIT;
static mut USB2_STATE: State = STATE_INIT;

unsafe fn state(usb: &ral::usb::RegisterBlock) -> &'static mut State {
    match &*usb as *const _ {
        ral::usb::USB1 => &mut USB1_STATE,
        ral::usb::USB2 => &mut USB2_STATE,
        _ => unreachable!("ral module ensures that the USB instance is one of these two value"),
    }
}

/// "Steal" the transfer descriptors for this USB state, and return an array of transfer
/// descriptor references.
///
/// # Safety
///
/// This should only be called once. You must make sure that the static, mutable references
/// aren't mutably aliased. Consider taking them from this collection, and assigning them
/// elsewhere.
unsafe fn steal_tds(usb: &ral::usb::RegisterBlock) -> [Option<&'static mut td::TD>; QH_COUNT] {
    let mut tds = [
        None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
        None,
    ];
    for (dst, src) in tds.iter_mut().zip(state(usb).tds.0.iter_mut()) {
        *dst = Some(src);
    }
    tds
}

/// "Steal" the queue heads for this USB state, and return an array of references to queue
/// heads
///
/// # Safety
///
/// This should only be called once. You must make sure that the static, mutable references
/// aren't mutably aliased. Consider taking them from this collection, and assigning them
/// elsewhere.
unsafe fn steal_qhs(usb: &ral::usb::RegisterBlock) -> [Option<&'static mut qh::QH>; QH_COUNT] {
    let mut qhs = [
        None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
        None,
    ];
    for (dst, src) in qhs.iter_mut().zip(state(usb).qhs.0.iter_mut()) {
        *dst = Some(src);
    }
    qhs
}

pub fn assign_endptlistaddr(usb: &ral::usb::RegisterBlock) {
    let ptr = unsafe { state(usb).qhs.0.as_ptr() };
    ral::write_reg!(ral::usb, usb, ASYNCLISTADDR, ptr as u32);
}

pub struct Allocator<U> {
    buffers: crate::buffer::Allocator,
    usb: &'static ral::usb::RegisterBlock,
    qhs: [Option<&'static mut qh::QH>; QH_COUNT],
    tds: [Option<&'static mut td::TD>; QH_COUNT],
    addresses: AddressAllocator,
    _core: core::marker::PhantomData<U>,
}

impl<U> Allocator<U>
where
    U: UsbCore<EndpointOut = super::Endpoint, EndpointIn = super::Endpoint>,
{
    /// Create a USB endpoint allocator
    ///
    /// # Safety
    ///
    /// You must make sure that there is only ever one allocator created
    /// in the program. The allocator hands out references to static, mutable
    /// memory. Creating multiple allocators will result in endpoints that
    /// refer to the same memory location.
    pub unsafe fn new(
        usb: &'static ral::usb::RegisterBlock,
        buffers: crate::buffer::Allocator,
    ) -> Self {
        let qhs = steal_qhs(&usb);
        let tds = steal_tds(&usb);
        Allocator {
            buffers,
            usb,
            qhs,
            tds,
            addresses: AddressAllocator::new(),
            _core: core::marker::PhantomData,
        }
    }

    /// Allocate an endpoint address
    fn alloc_addr(
        &mut self,
        dir: UsbDirection,
        config: &EndpointConfig,
    ) -> Result<EndpointAddress> {
        if let Some(addr) = config.fixed_address() {
            if !self.addresses.is_available(addr.number(), addr.direction()) {
                return Err(UsbError::EndpointUnavailable);
            }
            self.addresses.reserve(addr.number(), addr.direction());
            Ok(addr)
        } else {
            let number = self.addresses.next(dir).ok_or(UsbError::EndpointOverflow)?;
            Ok(EndpointAddress::from_parts(number, dir))
        }
    }

    fn alloc(
        &mut self,
        idx: usize,
        address: EndpointAddress,
        config: &EndpointConfig,
    ) -> Result<super::Endpoint> {
        let qh = self
            .qhs
            .get_mut(idx)
            .and_then(|qh| qh.take())
            .ok_or(UsbError::EndpointUnavailable)?;
        let td = self.tds[idx].take().unwrap();

        let buffer = self
            .buffers
            .allocate(config.max_packet_size().into())
            .ok_or(UsbError::EndpointMemoryOverflow)?;

        Ok(crate::usbcore::Endpoint::new(
            self.usb, address, qh, td, buffer,
        ))
    }
}

impl<U> UsbEndpointAllocator<U> for Allocator<U>
where
    U: UsbCore<EndpointOut = super::Endpoint, EndpointIn = super::Endpoint>,
{
    fn alloc_out(&mut self, config: &EndpointConfig) -> Result<U::EndpointOut> {
        let address = self.alloc_addr(UsbDirection::Out, config)?;
        self.alloc(2 * address.number() as usize, address, config)
    }
    fn alloc_in(&mut self, config: &EndpointConfig) -> Result<U::EndpointIn> {
        let address = self.alloc_addr(UsbDirection::Out, config)?;
        self.alloc((2 * address.number() as usize) + 1, address, config)
    }
    fn begin_interface(&mut self) -> Result<()> {
        Err(UsbError::Unsupported) // TODO?
    }
    fn next_alt_setting(&mut self) -> Result<()> {
        Err(UsbError::Unsupported) // TODO?
    }
}

/// Helper type to allocate endpoint numbers
struct AddressAllocator {
    mask_out: u8,
    mask_in: u8,
}

impl AddressAllocator {
    const fn new() -> Self {
        Self {
            mask_out: 0,
            mask_in: 0,
        }
    }
    fn is_available(&self, number: u8, dir: UsbDirection) -> bool {
        let mask = match dir {
            UsbDirection::In => &self.mask_in,
            UsbDirection::Out => &self.mask_out,
        };
        number < 8 && *mask & (1 << number) == 0
    }
    fn next(&mut self, dir: UsbDirection) -> Option<u8> {
        let mask = match dir {
            UsbDirection::In => &mut self.mask_in,
            UsbDirection::Out => &mut self.mask_out,
        };
        // EP0 can only be reserved
        let number = (*mask | 1).trailing_ones();
        let bit = 1u8.checked_shl(number)?;
        *mask |= bit;
        Some(number as u8)
    }
    fn reserve(&mut self, number: u8, dir: UsbDirection) {
        let mask = match dir {
            UsbDirection::In => &mut self.mask_in,
            UsbDirection::Out => &mut self.mask_out,
        };
        *mask |= 1 << number;
    }
}

#[cfg(test)]
mod test {
    use super::AddressAllocator;
    use endpoint_trait::UsbDirection;

    const OUT: UsbDirection = UsbDirection::Out;
    const IN: UsbDirection = UsbDirection::In;

    #[test]
    fn address_allocator() {
        let mut addr = AddressAllocator::new();
        for number in 1..8 {
            assert!(addr.is_available(number, OUT));
            assert_eq!(addr.next(OUT).unwrap(), number);
            assert!(!addr.is_available(number, OUT));
        }
        for number in 1..8 {
            assert!(addr.is_available(number, IN));
            assert_eq!(addr.next(IN).unwrap(), number);
            assert!(!addr.is_available(number, IN));
        }

        assert!(addr.next(OUT).is_none());
        assert!(addr.next(IN).is_none());

        assert!(addr.is_available(0, OUT));
        addr.reserve(0, OUT);
        assert!(!addr.is_available(0, OUT));

        assert!(addr.is_available(0, IN));
        addr.reserve(0, IN);
        assert!(!addr.is_available(0, IN));

        assert!(addr.next(OUT).is_none());
        assert!(addr.next(IN).is_none());
    }
}
