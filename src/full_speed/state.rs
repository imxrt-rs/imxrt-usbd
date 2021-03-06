//! Static state that's 'owned' by a USB instance

use crate::QH_COUNT;
use crate::{qh, ral, td};

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

unsafe fn state(usb: &ral::USB) -> &'static mut State {
    match usb.inst() {
        ral::Inst::One => &mut USB1_STATE,
        ral::Inst::Two => &mut USB2_STATE,
    }
}

/// Returns a pointer to the queue heads collection for this USB instance
///
/// This is only safe to use when assigning the ENDPTLISTADDR to the USB
/// instance.
pub fn assign_endptlistaddr(usb: &ral::USB) {
    let ptr = unsafe { state(usb).qhs.0.as_ptr() };
    ral::write_reg!(ral::usb, usb, ASYNCLISTADDR, ptr as u32);
}

/// "Steal" the queue heads for this USB state, and return an array of references to queue
/// heads
///
/// # Safety
///
/// This should only be called once. You must make sure that the static, mutable references
/// aren't mutably aliased. Consider taking them from this collection, and assigning them
/// elsewhere.
pub unsafe fn steal_qhs(usb: &ral::USB) -> [Option<&'static mut qh::QH>; QH_COUNT] {
    let mut qhs = [
        None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
        None,
    ];
    for (dst, src) in qhs.iter_mut().zip(state(usb).qhs.0.iter_mut()) {
        *dst = Some(src);
    }
    qhs
}

/// "Steal" the transfer descriptors for this USB state, and return an array of transfer
/// descriptor references.
///
/// # Safety
///
/// This should only be called once. You must make sure that the static, mutable references
/// aren't mutably aliased. Consider taking them from this collection, and assigning them
/// elsewhere.
pub unsafe fn steal_tds(usb: &ral::USB) -> [Option<&'static mut td::TD>; QH_COUNT] {
    let mut tds = [
        None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
        None,
    ];
    for (dst, src) in tds.iter_mut().zip(state(usb).tds.0.iter_mut()) {
        *dst = Some(src);
    }
    tds
}
