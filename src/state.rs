#![allow(clippy::declare_interior_mutable_const)] // Usage is legit in this module.

use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{buffer::Buffer, endpoint::Endpoint, qh::Qh, td::Td};
use usb_device::{
    endpoint::{EndpointAddress, EndpointType},
    UsbDirection,
};

/// A list of transfer descriptors
///
/// Supports 1 TD per QH (per endpoint direction)
#[repr(align(32))]
struct TdList<const COUNT: usize>([UnsafeCell<Td>; COUNT]);

impl<const COUNT: usize> TdList<COUNT> {
    const fn new() -> Self {
        const TD: UnsafeCell<Td> = UnsafeCell::new(Td::new());
        Self([TD; COUNT])
    }
}

/// A list of queue heads
///
/// One queue head per endpoint, per direction (default).
#[repr(align(4096))]
struct QhList<const COUNT: usize>([UnsafeCell<Qh>; COUNT]);

impl<const COUNT: usize> QhList<COUNT> {
    const fn new() -> Self {
        const QH: UnsafeCell<Qh> = UnsafeCell::new(Qh::new());
        Self([QH; COUNT])
    }
}

/// The collection of endpoints.
///
/// Maintained inside the EndpointState so that it's sized just right.
struct EpList<const COUNT: usize>([UnsafeCell<MaybeUninit<Endpoint>>; COUNT]);

impl<const COUNT: usize> EpList<COUNT> {
    const fn new() -> Self {
        const EP: UnsafeCell<MaybeUninit<Endpoint>> = UnsafeCell::new(MaybeUninit::uninit());
        Self([EP; COUNT])
    }
}

/// The maximum supported number of endpoints.
///
/// Eight endpoints, two in each direction. Any endpoints allocated
/// beyond this are wasted.
pub const MAX_ENDPOINTS: usize = 8 * 2;

/// Produces an index into the EPs, QHs, and TDs collections
fn index(ep_addr: EndpointAddress) -> usize {
    (ep_addr.index() * 2) + (UsbDirection::In == ep_addr.direction()) as usize
}

/// Driver state associated with endpoints.
///
/// Each USB driver needs an `EndpointState`. Allocate a `static` object
/// and supply it to your USB constructor. Make sure that states are not
/// shared across USB instances; otherwise, the driver constructor panics.
///
/// Use [`max_endpoints()`](EndpointState::max_endpoints) if you're not interested in reducing the
/// memory used by this allocation. The default object holds enough
/// state for all supported endpoints.
///
/// ```
/// use imxrt_usbd::EndpointState;
///
/// static EP_STATE: EndpointState = EndpointState::max_endpoints();
/// ```
///
/// If you know that you can use fewer endpoints, you can control the
/// memory utilization with the const generic `COUNT`. You're expected
/// to provide at least two endpoints -- one in each direction -- for
/// control endpoints.
///
/// Know that endpoints are allocated in pairs; all even endpoints are
/// OUT, and all odd endpoints are IN. For example, a `COUNT` of 5 will
/// have 3 out endpoints, and 2 in endpoints. You can never have more
/// IN that OUT endpoints without overallocating OUT endpoints.
///
/// ```
/// use imxrt_usbd::EndpointState;
///
/// static EP_STATE: EndpointState<5> = EndpointState::new();
/// ```
///
/// Any endpoint state allocated beyond [`MAX_ENDPOINTS`] are wasted.
pub struct EndpointState<const COUNT: usize = MAX_ENDPOINTS> {
    qh_list: QhList<COUNT>,
    td_list: TdList<COUNT>,
    ep_list: EpList<COUNT>,
    /// Low 16 bits are used for tracking endpoint allocation.
    /// Bit 31 is set when the allocator is first taken. This
    /// bit is always dropped during u32 -> u16 conversions.
    alloc_mask: AtomicU32,
}

unsafe impl<const COUNT: usize> Sync for EndpointState<COUNT> {}

impl EndpointState<MAX_ENDPOINTS> {
    /// Allocate space for the maximum number of endpoints.
    ///
    /// Use this if you don't want to consider the exact number
    /// of endpoints that you might need.
    pub const fn max_endpoints() -> Self {
        Self::new()
    }
}

impl<const COUNT: usize> EndpointState<COUNT> {
    /// Allocate state for `COUNT` endpoints.
    pub const fn new() -> Self {
        Self {
            qh_list: QhList::new(),
            td_list: TdList::new(),
            ep_list: EpList::new(),
            alloc_mask: AtomicU32::new(0),
        }
    }

    /// Acquire the allocator.
    ///
    /// Returns `None` if the allocator was already taken.
    pub(crate) fn allocator(&self) -> Option<EndpointAllocator> {
        const ALLOCATOR_TAKEN: u32 = 1 << 31;
        let alloc_mask = self.alloc_mask.fetch_or(ALLOCATOR_TAKEN, Ordering::SeqCst);
        (alloc_mask & ALLOCATOR_TAKEN == 0).then(|| EndpointAllocator {
            qh_list: &self.qh_list.0[..self.qh_list.0.len().min(MAX_ENDPOINTS)],
            td_list: &self.td_list.0[..self.td_list.0.len().min(MAX_ENDPOINTS)],
            ep_list: &self.ep_list.0[..self.ep_list.0.len().min(MAX_ENDPOINTS)],
            alloc_mask: &self.alloc_mask,
        })
    }
}

pub struct EndpointAllocator<'a> {
    qh_list: &'a [UnsafeCell<Qh>],
    td_list: &'a [UnsafeCell<Td>],
    ep_list: &'a [UnsafeCell<MaybeUninit<Endpoint>>],
    alloc_mask: &'a AtomicU32,
}

unsafe impl Send for EndpointAllocator<'_> {}

impl EndpointAllocator<'_> {
    /// Atomically inserts the endpoint bit into the allocation mask, returning `None` if the
    /// bit was already set.
    fn try_mask_update(&mut self, mask: u16) -> Option<()> {
        let mask = mask.into();
        (mask & self.alloc_mask.fetch_or(mask, Ordering::SeqCst) == 0).then_some(())
    }

    /// Returns `Some` if the endpoint is allocated.
    fn check_allocated(&self, index: usize) -> Option<()> {
        let mask = (index < self.qh_list.len()).then_some(1u16 << index)?;
        (mask & self.alloc_mask.load(Ordering::SeqCst) as u16 != 0).then_some(())
    }

    /// Acquire the QH list address.
    ///
    /// Used to tell the hardware where the queue heads are located.
    pub fn qh_list_addr(&self) -> *const () {
        self.qh_list.as_ptr().cast()
    }

    /// Returns the total number of endpoints that could be allocated.
    pub fn capacity(&self) -> usize {
        self.ep_list.len()
    }

    /// Acquire the endpoint.
    ///
    /// Returns `None` if the endpoint isn't allocated.
    pub fn endpoint(&self, addr: EndpointAddress) -> Option<&Endpoint> {
        let index = index(addr);
        self.check_allocated(index)?;

        // Safety: there's no other mutable access at this call site.
        // Perceived lifetime is tied to the EndpointAllocator, which has an
        // immutable receiver.

        let ep = unsafe { &*self.ep_list[index].get() };
        // Safety: endpoint is allocated. Checked above.
        Some(unsafe { ep.assume_init_ref() })
    }

    /// Aquire the mutable endpoint.
    ///
    /// Returns `None` if the endpoint isn't allocated.
    pub fn endpoint_mut(&mut self, addr: EndpointAddress) -> Option<&mut Endpoint> {
        let index = index(addr);
        self.check_allocated(index)?;

        // Safety: there's no other immutable or mutable access at this call site.
        // Perceived lifetime is tied to the EndpointAllocator, which has a
        // mutable receiver.
        let ep = unsafe { &mut *self.ep_list[index].get() };

        // Safety: endpoint is allocated. Checked above.
        Some(unsafe { ep.assume_init_mut() })
    }

    /// Allocate the endpoint for the specified address.
    ///
    /// Returns `None` if any are true:
    ///
    /// - The endpoint is already allocated.
    /// - We cannot allocate an endpoint for the given address.
    pub fn allocate_endpoint(
        &mut self,
        addr: EndpointAddress,
        buffer: Buffer,
        kind: EndpointType,
    ) -> Option<&mut Endpoint> {
        let index = index(addr);
        let mask = (index < self.qh_list.len()).then_some(1u16 << index)?;

        // If we pass this call, we're the only caller able to observe mutable
        // QHs, TDs, and EPs at index.
        self.try_mask_update(mask)?;

        // Safety: index in range. Atomic update on alloc_mask prevents races for
        // allocation, and ensures that we only release one &mut reference for each
        // component.
        let qh = unsafe { &mut *self.qh_list[index].get() };
        let td = unsafe { &mut *self.td_list[index].get() };
        // We cannot access these two components after this call. The endpoint
        // takes mutable references, so it has exclusive ownership of both.
        // This module is designed to isolate this access so we can visually
        // see where we have these &mut accesses.

        // EP is uninitialized.
        let ep = unsafe { &mut *self.ep_list[index].get() };
        // Nothing to drop here.
        ep.write(Endpoint::new(addr, qh, td, buffer, kind));
        // Safety: EP is initialized.
        Some(unsafe { ep.assume_init_mut() })
    }
}

#[cfg(test)]
mod tests {
    use super::{EndpointAddress, EndpointState, EndpointType};
    use crate::buffer;

    #[test]
    fn acquire_allocator() {
        let ep_state = EndpointState::max_endpoints();
        ep_state.allocator().unwrap();
        for _ in 0..10 {
            assert!(ep_state.allocator().is_none());
        }
    }

    #[test]
    fn allocate_endpoint() {
        let mut buffer = [0; 128];
        let mut buffer_alloc = unsafe { buffer::Allocator::from_buffer(&mut buffer) };
        let ep_state = EndpointState::max_endpoints();
        let mut ep_alloc = ep_state.allocator().unwrap();

        // First endpoint allocation.
        let addr = EndpointAddress::from(0);
        assert!(ep_alloc.endpoint(addr).is_none());
        assert!(ep_alloc.endpoint_mut(addr).is_none());

        let ep = ep_alloc
            .allocate_endpoint(addr, buffer_alloc.allocate(2).unwrap(), EndpointType::Bulk)
            .unwrap();
        assert_eq!(ep.address(), addr);

        assert!(ep_alloc.endpoint(addr).is_some());
        assert!(ep_alloc.endpoint_mut(addr).is_some());

        // Double-allocate existing endpoint.
        let ep =
            ep_alloc.allocate_endpoint(addr, buffer_alloc.allocate(2).unwrap(), EndpointType::Bulk);
        assert!(ep.is_none());

        assert!(ep_alloc.endpoint(addr).is_some());
        assert!(ep_alloc.endpoint_mut(addr).is_some());

        // Allocate a new endpoint.
        let addr = EndpointAddress::from(1 << 7);

        assert!(ep_alloc.endpoint(addr).is_none());
        assert!(ep_alloc.endpoint_mut(addr).is_none());

        let ep = ep_alloc
            .allocate_endpoint(addr, buffer_alloc.allocate(2).unwrap(), EndpointType::Bulk)
            .unwrap();
        assert_eq!(ep.address(), addr);
    }
}
