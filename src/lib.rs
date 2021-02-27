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

    /// Initialize all USB physical, analog clocks, and core registers.
    /// Assumes that the CCM clock gates are enabled.
    ///
    /// You **must** call this once, before creating the complete USB
    /// bus.
    pub fn initialize(&mut self, ccm_analog: &ral::ccm_analog::Instance) {
        pll::initialize(ccm_analog);

        ral::write_reg!(ral::usbphy, self.phy, CTRL_SET, SFTRST: 1);
        ral::write_reg!(ral::usbphy, self.phy, CTRL_CLR, SFTRST: 1);
        ral::write_reg!(ral::usbphy, self.phy, CTRL_CLR, CLKGATE: 1);
        ral::write_reg!(ral::usbphy, self.phy, PWD, 0);

        ral::modify_reg!(ral::usb, self.usb, USBCMD, RST: 1);
        while ral::read_reg!(ral::usb, self.usb, USBCMD, RST == 1) {}

        ral::write_reg!(ral::usb, self.usb, USBMODE, CM: CM_2, SLOM: 1);

        // This forces the bus to run at full speed, not high speed. Specifically,
        // it disables the chirp. If you're interested in playing with a high-speed
        // USB driver, you'll want to remove this line, or clear PFSC.
        ral::modify_reg!(ral::usb, self.usb, PORTSC1, PFSC: 1);

        ral::modify_reg!(ral::usb, self.usb, USBSTS, |usbsts| usbsts);
        ral::write_reg!(ral::usb, self.usb, USBINTR, 0);

        ral::write_reg!(
            ral::usb,
            self.usb,
            ASYNCLISTADDR,
            self.qhs.as_ptr() as *const _ as u32
        );
    }

    fn set_address(&mut self, address: u8) {
        // See the "quirk" note in the UsbBus impl. We're using USBADRA to let
        // the hardware set the address before the status phase.
        ral::write_reg!(ral::usb, self.usb, DEVICEADDR, USBADR: address as u32, USBADRA: 1);
    }

    fn attach(&mut self) {
        ral::write_reg!(ral::usb, self.usb, USBCMD, RS: 1);
    }

    fn bus_reset(&mut self) {
        ral::modify_reg!(ral::usb, self.usb, ENDPTSTAT, |endptstat| endptstat);

        ral::modify_reg!(ral::usb, self.usb, ENDPTCOMPLETE, |endptcomplete| {
            endptcomplete
        });
        ral::modify_reg!(ral::usb, self.usb, ENDPTNAK, |endptnak| endptnak);
        ral::write_reg!(ral::usb, self.usb, ENDPTNAKEN, 0);

        while ral::read_reg!(ral::usb, self.usb, ENDPTPRIME) != 0 {}
        ral::write_reg!(ral::usb, self.usb, ENDPTFLUSH, u32::max_value());
        while ral::read_reg!(ral::usb, self.usb, ENDPTFLUSH) != 0 {}

        debug_assert!(
            ral::read_reg!(ral::usb, self.usb, PORTSC1, PR == 1),
            "Took too long to handle bus reset"
        );
    }
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
