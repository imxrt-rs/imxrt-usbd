//! A USB driver for i.MX RT processors
//!
//! `imxrt-usb` provides a [`usb-device`] USB bus implementation, allowing you
//! to add USB device features to your embedded Rust program. The package
//! supports all of the i.MX RT chips available in the [`imxrt-ral`] register
//! access layer.
//!
//! # Build
//!
//! `imxrt-usb` will not build in isolation. It requires that an [`imxrt-ral`]
//! chip-specific feature is enabled in your dependency chain. If that `imxrt-ral`
//! feature is *any* of the following features,
//!
//! - `"imxrt1051"`
//! - `"imxrt1052"`
//! - `"imxrt1061"`
//! - `"imxrt1062"`
//!
//! then you **must** enable this crate's `"double-instance"` feature to properly
//! support the two available USB instances. Failure to specify features will
//! result in a failed build.
//!
//! # Usage
//!
//! This library currently focuses on the `usb-device` USB bus implementation,
//! so there are not many i.MX RT device-specific features exposed from this
//! API. Here's how to get started with `usb-device` support.
//!
//! 1. Depend on this crate, the `usb-device` crate, and a USB class crate that
//!    supports `usb-device`.
//! 2. Instantiate a [`USB`](USB) driver from the `imxrt-ral` USB instances. See
//!    the `USB` docs for more information.
//! 3. Wrap your `USB` instance in a [`BusAdapter`](BusAdapter), which implements the USB bus
//!    trait
//! 4. Supply your `BusAdapter` to the `usb-device` devices.
//!
//! See the [`USB`] and [`BusAdapter`] documentation for requirements and examples.
//!
//! [`imxrt-ral`]: https://crates.io/crates/imxrt-ral
//! [`usb-device`]: https://crates.io/crates/usb-device

#![no_std]

#[macro_use]
mod log;

mod buffer;
mod bus;
mod endpoint;
mod pll;
mod qh;
mod ral;
mod td;
mod vcell;

pub use bus::BusAdapter;

use endpoint::{Endpoint, Status};
use usb_device::{
    endpoint::{EndpointAddress, EndpointType},
    UsbDirection,
};

const EP_INIT: [Option<Endpoint>; QH_COUNT] = [
    None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
];

/// Produces an index into the EPs, QHs, and TDs collections
fn index(ep_addr: EndpointAddress) -> usize {
    (ep_addr.index() * 2) + (UsbDirection::In == ep_addr.direction()) as usize
}

/// A USB driver
///
/// `USB` itself doesn't provide much of an API. After you allocate a `USB` with [`new()`](USB::new),
/// you must
///
/// - call [`initialize()`](USB::initialize) once
/// - supply endpoint memory with [`set_endpoint_memory()`](USB::set_endpoint_memory)
///
/// After that, you should wrap it with a [`BusAdapter`](crate::BusAdapter), and combine the bus
/// with the `usb_device` APIs.
///
/// # Example
///
/// This example shows a bare-minimum setup for the USB driver on an i.MX RT 1062 processor.
///
/// ```no_run
/// use imxrt_usb::USB;
/// use imxrt_ral::{usb, usbphy, ccm, ccm_analog};
///
/// static mut ENDPOINT_MEMORY: [u8; 1024] = [0; 1024];
///
/// let mut usb = USB::new(
///     usb::USB1::take().unwrap(),
///     usbphy::USBPHY1::take().unwrap(),
/// );
///
/// let ccm_analog = ccm_analog::CCM_ANALOG::take().unwrap();
/// let ccm = ccm::CCM::take().unwrap();
///
/// // Enable the USB1 clock gates
/// imxrt_ral::modify_reg!(ccm, ccm, CCGR6, CG1: 0b11, CG0: 0b11);
/// usb.initialize(&ccm_analog);
///
/// unsafe {
///     // Safety: guarantee that we won't use ENDPOINT_MEMORY
///     // for anything else.
///     usb.set_endpoint_memory(&mut ENDPOINT_MEMORY);
/// }
///
/// // Construct the Bus...
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
    pub fn new(usb: ral::usb::Instance, phy: ral::usbphy::Instance) -> Self {
        // Safety: taking static memory. Assumes that the provided
        // USB instance is a singleton, which is the only safe way for it
        // to exist.
        unsafe {
            let (qhs, tds) = match (&*usb as *const _, &*phy as *const _) {
                (ral::usb::USB1, ral::usbphy::USBPHY1) => {
                    (USB1_STATE.steal_qhs(), USB1_STATE.steal_tds())
                }
                #[cfg(feature = "double-instance")]
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

    /// Set the region of memory that can be used for endpoint I/O
    ///
    /// This memory will be shared across all endpoints. you should size it
    /// to support all the endpoints that might be allocated by your USB classes.
    ///
    /// After this call, `USB` assumes that it's the sole owner of `buffer`.
    /// You assume the `unsafe`ty to make that true.
    pub fn set_endpoint_memory(&mut self, buffer: &'static mut [u8]) {
        self.buffer_allocator = buffer::Allocator::new(buffer);
    }

    /// Initialize the USB physical layer, the analog clocks, and the USB
    /// core registers
    ///
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

    /// Check if the endpoint is valid
    fn is_allocated(&self, addr: EndpointAddress) -> bool {
        self.endpoints
            .get(index(addr))
            .map(|ep| ep.is_some())
            .unwrap_or(false)
    }

    /// Read either a setup, or a data buffer, from EP0 OUT
    ///
    /// Return the status if there's a pending transaction, or an error.
    ///
    /// # Panics
    ///
    /// Panics if EP0 OUT isn't allocated.
    fn ctrl0_read(&mut self, buffer: &mut [u8]) -> Result<usize, Status> {
        let ctrl_out = self.endpoints[0].as_mut().unwrap();
        if ctrl_out.has_setup(&self.usb) && buffer.len() >= 8 {
            let setup = ctrl_out.read_setup(&self.usb);
            buffer[..8].copy_from_slice(&setup.to_le_bytes());

            ctrl_out.flush(&self.usb);
            ctrl_out.clear_complete(&self.usb);
            let max_packet_len = ctrl_out.max_packet_len();
            ctrl_out.schedule_transfer(&self.usb, max_packet_len);

            return Ok(8);
        } else {
            ctrl_out.check_status()?;

            ctrl_out.clear_complete(&self.usb);
            ctrl_out.clear_nack(&self.usb);

            let read = ctrl_out.read(buffer);
            let max_packet_len = ctrl_out.max_packet_len();
            ctrl_out.schedule_transfer(&self.usb, max_packet_len);

            Ok(read)
        }
    }

    /// Write to the host from EP0 IN
    ///
    /// Schedules the next OUT transfer to satisfy a status phase.
    ///
    /// # Panics
    ///
    /// Panics if EP0 IN isn't allocated, or if EP0 OUT isn't allocated.
    fn ctrl0_write(&mut self, buffer: &[u8]) -> Result<usize, Status> {
        let ctrl_in = self.endpoints[1].as_mut().unwrap();
        ctrl_in.check_status()?;

        ctrl_in.clear_nack(&self.usb);

        let written = ctrl_in.write(buffer);
        ctrl_in.schedule_transfer(&self.usb, written);

        if !buffer.is_empty() {
            // Schedule an OUT transfer for the status phase...
            let ctrl_out = self.endpoints[0].as_mut().unwrap();
            ctrl_out.flush(&self.usb);
            ctrl_out.clear_complete(&self.usb);
            ctrl_out.clear_nack(&self.usb);
            ctrl_out.schedule_transfer(&self.usb, 0);
        }

        Ok(written)
    }

    /// Read data from an endpoint, and schedule the next transfer
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated.
    fn ep_read(&mut self, buffer: &mut [u8], addr: EndpointAddress) -> Result<usize, Status> {
        let ep = self.endpoints[index(addr)].as_mut().unwrap();
        ep.check_status()?;

        ep.clear_complete(&self.usb);
        ep.clear_nack(&self.usb);

        let read = ep.read(buffer);

        let max_packet_len = ep.max_packet_len();
        ep.schedule_transfer(&self.usb, max_packet_len);

        Ok(read)
    }

    /// Write data to an endpoint
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated.
    fn ep_write(&mut self, buffer: &[u8], addr: EndpointAddress) -> Result<usize, Status> {
        let ep = self.endpoints[index(addr)].as_mut().unwrap();
        ep.check_status()?;

        ep.clear_nack(&self.usb);

        let written = ep.write(buffer);
        ep.schedule_transfer(&self.usb, written);

        Ok(written)
    }

    /// Stall an endpoint
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated
    fn ep_stall(&mut self, stall: bool, addr: EndpointAddress) {
        self.endpoints[index(addr)]
            .as_mut()
            .unwrap()
            .set_stalled(&self.usb, stall);
    }

    /// Checks if an endpoint is stalled
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated
    fn is_ep_stalled(&self, addr: EndpointAddress) -> bool {
        self.endpoints[index(addr)]
            .as_ref()
            .unwrap()
            .is_stalled(&self.usb)
    }

    /// Allocate a buffer from the endpoint memory
    fn allocate_buffer(&mut self, max_packet_len: usize) -> Option<buffer::Buffer> {
        self.buffer_allocator.allocate(max_packet_len)
    }

    /// Allocate a specific endpoint
    ///
    /// # Panics
    ///
    /// Panics if the endpoint is already allocated.
    fn allocate_ep(&mut self, addr: EndpointAddress, buffer: buffer::Buffer, kind: EndpointType) {
        let qh = self.qhs[index(addr)].take().unwrap();
        let td = self.tds[index(addr)].take().unwrap();

        qh.set_max_packet_len(buffer.len());
        qh.set_zero_length_termination(false);
        qh.set_interrupt_on_setup(
            EndpointType::Control == kind && addr.direction() == UsbDirection::Out,
        );

        td.set_terminate();
        td.clear_status();

        let mut ep = Endpoint::new(addr, qh, td, buffer);
        ep.initialize(&self.usb, kind);
        self.endpoints[index(addr)] = Some(ep);
    }

    /// Enable all non-zero endpoints, and schedule OUT transfers
    ///
    /// This should only be called when the device is configured
    fn enable_endpoints(&mut self) {
        for ep in self.endpoints.iter_mut().flat_map(core::convert::identity) {
            ep.enable(&self.usb);
            if ep.address().direction() == UsbDirection::Out {
                let max_packet_len = ep.max_packet_len();
                ep.schedule_transfer(&self.usb, max_packet_len);
            }
        }
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
#[cfg(feature = "double-instance")]
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
                #[cfg(feature = "double-instance")]
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
