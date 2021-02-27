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

use endpoint::Endpoint;
use imxrt_ral as ral;

const EP_INIT: [Option<Endpoint>; QH_COUNT] = [
    None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
];

/// A USB driver
///
/// `USB` itself doesn't provide much of an API. After you allocate a `USB` with [`new()`](USB::new),
/// you must
///
/// - call [`initialize()`](USB::initialize) once
/// - supply endpoint memory with [`set_endpoint_memory()`](USB::set_endpoint_memory)
///
/// After that, you should wrap it with a [`Bus`](crate::Bus), and combine the bus with the `usb_device`
/// interfaces.
///
/// # Example
///
/// This example shows a bare-minimum setup for the USB driver.
///
/// ```no_run
/// use imxrt_usb::USB;
/// use imxrt_ral::{usb, usbphy, ccm_analog};
///
/// static mut ENDPOINT_MEMORY: [u8; 1024] = [0; 1024];
///
/// let mut usb = USB::new(
///     usb::USB1::take().unwrap(),
///     usbphy::USBPHY1::take().unwrap(),
/// );
///
/// let ccm_analog = ccm_analog::CCM_ANALOG::take().unwrap();
/// usb.initialize(&ccm_analog);
///
/// unsafe {
///     usb.set_endpoint_memory(&mut ENDPOINT_MEMORY);
/// }
pub struct USB {
    endpoints: [Option<Endpoint>; QH_COUNT],
    usb: ral::usb::Instance,
    phy: ral::usbphy::Instance,
    qhs: [Option<&'static mut qh::QH>; QH_COUNT],
    tds: [Option<&'static mut td::TD>; QH_COUNT],
    buffer_allocator: buffer::Allocator,
}

impl USB {
    /// Create a new `USB` driver
    ///
    /// Creation does nothing except for assign static memory to the driver.
    /// After creating the driver, call [`initialize()`](USB::initialize).
    ///
    /// # Panics
    ///
    /// Panics if the `usb` instance and the `phy` instances are mismatched.
    ///
    /// # Safety
    ///
    /// `new` is safe, since there is only one safe way to obtain the two
    /// required instances from the RAL. But, if you use the RAL unsafely,
    /// the behavior in `USB` is undefined.
    pub fn new(usb: ral::usb::Instance, phy: ral::usbphy::Instance) -> Self {
        // Safety: taking static memory. Assumes that the provided
        // USB instance is a singleton, which is the only safe way for it
        // to exist.
        unsafe {
            let (qhs, tds) = match (&*usb as *const _, &*phy as *const _) {
                (ral::usb::USB1, ral::usbphy::USBPHY1) => {
                    (USB1_STATE.steal_qhs(), USB1_STATE.steal_tds())
                }
                (ral::usb::USB2, ral::usbphy::USBPHY2) => {
                    (USB2_STATE.steal_qhs(), USB2_STATE.steal_tds())
                }
                _ => panic!("Mismatch USB and USBPHY"),
            };
            USB {
                endpoints: EP_INIT,
                usb,
                phy,
                qhs,
                tds,
                buffer_allocator: buffer::Allocator::empty(),
            }
        }
    }

    /// Set the region of memory that can be used for transfers with endpoints
    pub fn set_endpoint_memory(&mut self, buffer: &'static mut [u8]) {
        self.buffer_allocator = buffer::Allocator::new(buffer);
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

        State::assign_endptlistaddr(&self.usb);
    }

    fn set_address(&mut self, address: u8) {
        // See the "quirk" note in the UsbBus impl. We're using USBADRA to let
        // the hardware set the address before the status phase.
        ral::write_reg!(ral::usb, self.usb, DEVICEADDR, USBADR: address as u32, USBADRA: 1);
    }

    fn attach(&mut self) {
        // TODO should probably be a modify...
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

/// Just a helper type for static initialization
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

impl State {
    /// Returns a pointer to the queue heads collection for this USB instance
    ///
    /// This is only safe to use when assigning the ENDPTLISTADDR to the USB
    /// instance.
    fn assign_endptlistaddr(usb: &ral::usb::Instance) {
        let ptr = unsafe {
            match &**usb as *const _ {
                ral::usb::USB1 => USB1_STATE.qhs.0.as_ptr(),
                ral::usb::USB2 => USB2_STATE.qhs.0.as_ptr(),
                _ => panic!("Unhandled USB instance"),
            }
        };
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
    unsafe fn steal_qhs(&'static mut self) -> [Option<&'static mut qh::QH>; QH_COUNT] {
        let mut qhs = [
            None, None, None, None, None, None, None, None, None, None, None, None, None, None,
            None, None,
        ];
        for (dst, src) in qhs.iter_mut().zip(self.qhs.0.iter_mut()) {
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
    unsafe fn steal_tds(&'static mut self) -> [Option<&'static mut td::TD>; QH_COUNT] {
        let mut tds = [
            None, None, None, None, None, None, None, None, None, None, None, None, None, None,
            None, None,
        ];
        for (dst, src) in tds.iter_mut().zip(self.tds.0.iter_mut()) {
            *dst = Some(src);
        }
        tds
    }
}
