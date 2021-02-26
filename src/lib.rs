#![no_std]

#[macro_use]
mod log;

mod buffer;
mod bus;
mod endpoint;
mod pll;
mod qh;
mod td;
mod vcell;

pub use bus::Bus;

use imxrt_ral as ral;

type Endpoint = endpoint::Endpoint<'static>;

const EP_INIT: [Option<Endpoint>; QH_COUNT] = [
    None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
];

pub struct USB {
    endpoints: [Option<Endpoint>; QH_COUNT],
    usb: ral::usb::Instance,
    phy: ral::usbphy::Instance,
    qhs: &'static [qh::QH; QH_COUNT],
    tds: &'static [td::TD; QH_COUNT],
    buffer_allocator: buffer::Allocator,
}

impl USB {
    pub fn new(usb: ral::usb::Instance, phy: ral::usbphy::Instance) -> Self {
        // Safety: taking static memory. Assumes that the provided
        // USB instance is a singleton, which is the only safe way for it
        // to exist.
        match (&*usb as *const _, &*phy as *const _) {
            (ral::usb::USB1, ral::usbphy::USBPHY1) => USB {
                usb,
                phy,
                endpoints: EP_INIT,
                qhs: &USB1_STATE.qhs.0,
                tds: &USB1_STATE.tds.0,
                buffer_allocator: buffer::Allocator::empty(),
            },
            (ral::usb::USB2, ral::usbphy::USBPHY2) => USB {
                usb,
                phy,
                endpoints: EP_INIT,
                qhs: &USB2_STATE.qhs.0,
                tds: &USB2_STATE.tds.0,
                buffer_allocator: buffer::Allocator::empty(),
            },
            _ => panic!("Mismatch USB and USBPHY"),
        }
    }

    /// Set the region of memory that can be used for transfers with endpoints
    ///
    /// # Safety
    ///
    /// Caller must ensure that `memory` is valid for the `size` bytes. Caller must ensure that
    /// the allocation isn't used anywhere else.
    pub unsafe fn set_endpoint_memory(&mut self, memory: core::ptr::NonNull<u8>, size: usize) {
        self.buffer_allocator = buffer::Allocator::new(memory, size);
    }

    pub fn initialize(&mut self, ccm_analog: &ral::ccm_analog::Instance) {
        pll::initialize(ccm_analog);
        initialize(&self.usb, &self.phy, ccm_analog);
        set_enpoint_list_address(&self.usb, self.qhs.as_ptr() as *const _);
    }

    fn set_address(&mut self, address: u8) {
        set_address(&self.usb, address);
    }

    fn attach(&mut self) {
        attach(&self.usb);
    }

    fn bus_reset(&mut self) {
        bus_reset(&self.usb);
    }
}

//
// Helpers
//

/// Initialize all USB physical, analog clocks, and core registers.
/// Assumes that the CCM clock gates are enabled.
fn initialize(
    usb: &ral::usb::Instance,
    phy: &ral::usbphy::Instance,
    ccm_analog: &ral::ccm_analog::Instance,
) {
    pll::initialize(ccm_analog);

    ral::write_reg!(ral::usbphy, phy, CTRL_SET, SFTRST: 1);
    ral::write_reg!(ral::usbphy, phy, CTRL_CLR, SFTRST: 1);
    ral::write_reg!(ral::usbphy, phy, CTRL_CLR, CLKGATE: 1);
    ral::write_reg!(ral::usbphy, phy, PWD, 0);

    ral::modify_reg!(ral::usb, usb, USBCMD, RST: 1);
    while ral::read_reg!(ral::usb, usb, USBCMD, RST == 1) {}

    ral::write_reg!(ral::usb, usb, USBMODE, CM: CM_2, SLOM: 1);
    ral::modify_reg!(ral::usb, usb, PORTSC1, PFSC: 1);
    ral::modify_reg!(ral::usb, usb, USBSTS, |usbsts| usbsts);
    ral::write_reg!(ral::usb, usb, USBINTR, 0);
}

fn set_address(usb: &ral::usb::Instance, address: u8) {
    ral::write_reg!(ral::usb, usb, DEVICEADDR, USBADR: address as u32, USBADRA: 1);
}

fn set_enpoint_list_address(usb: &ral::usb::Instance, eplistaddr: *const ()) {
    ral::write_reg!(ral::usb, usb, ASYNCLISTADDR, eplistaddr as u32);
}

fn attach(usb: &ral::usb::Instance) {
    ral::write_reg!(ral::usb, usb, USBCMD, RS: 1);
}

fn bus_reset(usb: &ral::usb::Instance) {
    ral::modify_reg!(ral::usb, usb, ENDPTSTAT, |endptstat| endptstat);

    ral::modify_reg!(ral::usb, usb, ENDPTCOMPLETE, |endptcomplete| {
        endptcomplete
    });
    ral::modify_reg!(ral::usb, usb, ENDPTNAK, |endptnak| endptnak);
    ral::write_reg!(ral::usb, usb, ENDPTNAKEN, 0);

    while ral::read_reg!(ral::usb, usb, ENDPTPRIME) != 0 {}
    ral::write_reg!(ral::usb, usb, ENDPTFLUSH, u32::max_value());
    while ral::read_reg!(ral::usb, usb, ENDPTFLUSH) != 0 {}

    debug_assert!(
        ral::read_reg!(ral::usb, usb, PORTSC1, PR == 1),
        "Took too long to handle bus reset"
    );
}

//
// Static memory
//

/// Eight endpoints, two directions
const QH_COUNT: usize = 8 * 2;

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

/// USB driver state
struct State {
    qhs: QHList,
    tds: TDList,
}

const STATE_INIT: State = State {
    qhs: QH_LIST_INIT,
    tds: TD_LIST_INIT,
};

static USB1_STATE: State = STATE_INIT;
static USB2_STATE: State = STATE_INIT;

unsafe impl Send for qh::QH {}
unsafe impl Send for td::TD {}

unsafe impl Sync for qh::QH {}
unsafe impl Sync for td::TD {}
