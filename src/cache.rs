//! Cache maintenance operations
//!
//! These functions were adapted from the cortex-m (0.7.1) crate.
//! cortex-m only lets you access these functions when you have
//! the SCB in cortex_m::Peripherals collection. But, we neither want
//! to steal the peripheral(s), nor own them. So, we're duplicating
//! the routines that we need, and making sure that we're using
//! them safely.
//!
//! cortex-m crate available at <https://github.com/rust-embedded/cortex-m>.
//!
//! See <https://github.com/rust-embedded/cortex-m/issues/304>, and also #239.
//!
//! <https://github.com/rust-embedded/cortex-m/pull/320> indicates that this might
//! be available in a near-future cortex-m crate.

/// Cleans and invalidates D-cache by address.
///
/// * `addr`: The address to clean and invalidate.
/// * `size`: The number of bytes to clean and invalidate.
///
/// Cleans and invalidates D-cache starting from the first cache line containing `addr`,
/// finishing once at least `size` bytes have been cleaned and invalidated.
///
/// It is recommended that `addr` is aligned to the cache line size and `size` is a multiple of
/// the cache line size, otherwise surrounding data will also be cleaned.
///
/// Cleaning and invalidating causes data in the D-cache to be written back to main memory,
/// and then marks that data in the D-cache as invalid, causing future reads to first fetch
/// from main memory.
pub fn clean_invalidate_dcache_by_address(addr: usize, size: usize) {
    // No-op zero sized operations
    if size == 0 {
        return;
    }

    // Safety: write-only registers, pointer to static memory
    let cbp = unsafe { &*cortex_m::peripheral::CBP::PTR };

    cortex_m::asm::dsb();

    // Cache lines are fixed to 32 bit on Cortex-M7 and not present in earlier Cortex-M
    const LINESIZE: usize = 32;
    let num_lines = ((size - 1) / LINESIZE) + 1;

    let mut addr = addr & 0xFFFF_FFE0;

    for _ in 0..num_lines {
        // Safety: write to Cortex-M write-only register
        unsafe { cbp.dccimvac.write(addr as u32) };
        addr += LINESIZE;
    }

    cortex_m::asm::dsb();
    cortex_m::asm::isb();
}
