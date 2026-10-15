Changelog
=========

[Unreleased]
------------

Fix a panic in `UsbBus::is_stalled` when the host names an endpoint that the
device did not allocate (a `GET_STATUS` request addressed to an endpoint).
The bus now reports such an endpoint as not stalled.

Fix the D-cache maintenance that `schedule_transfer` performs on an endpoint's
queue head and transfer descriptor. It cleaned and invalidated the cache line
of a stack temporary rather than the descriptor itself, so a descriptor in
cacheable memory was never written back. State placed in non-cacheable memory
was not affected.

Implement `UsbBus::force_reset`, with or without the `transfer` feature. It
clears Run/Stop to drop the D+ pull-up, holds the disconnect for a fixed
busy-wait (about 10 ms at 600 MHz), then reattaches, so the host re-enumerates
the device. It previously returned `UsbError::Unsupported`.

Add the `transfer` feature: a per-endpoint queue of sized dTD-chain transfers
for bulk/interrupt endpoints.

- `BusAdapter::submit_write` / `submit_read` queue zero-copy IN/OUT transfers.
- `BusAdapter::poll_transfer` retires the oldest completed transfer.
- `BusAdapter::pending_transfers` counts in-flight transfers on an endpoint.
- `BusAdapter::cancel_transfers` drops all queued and in-flight transfers on an
  endpoint, for class-level reconfiguration.
- `BusAdapter::set_packet_queue_depth` configures eager OUT packet read-ahead.
- The packet path (`UsbBus::read` / `write`) is re-expressed over staging
  buffers under the feature; without the feature the packet path is unchanged
  from the previous release.
- Bus reset and endpoint enable flush the software-side transfer queue on all
  non-control endpoints (feature-gated).

[0.4.2] 2026-06-09
------------------

Don't re-prime a disabled OUT endpoint on unstall.

[0.4.1] 2026-05-16
------------------

Fix the `defmt` feature. Activate the `defmt` feature in usb-device, since this
package may try to log usb-device items.

[0.4.0] 2026-04-12
------------------

The package now requires imxrt-ral to represent its register access layer.
The `Peripherals` trait is gone, and users are expected to use imxrt-ral
peripheral instances.

The `defmt` feature replaces the `defmt-03` feature. Activating this
package's `defmt` feature _does not_ activate the similarly-named feature
in the usb-device package. You may enable them individually.

Upgrade package to Rust 2024.

[0.3.0] 2024-05-29
------------------

**BREAKING** Update to `usb-device` 0.3. By adopting this release, you're
required to use `usb-device` 0.3 and its compatible packages.

Add feature `defmt-03` to enable internal logging of the USB device stack.

[0.2.2] 2023-09-15
------------------

Resolve an issue where endpoints were reset when exiting suspend.

[0.2.1] 2023-03-14
------------------

Fix an overflowing left shift that could occur when enabling and allocating
endpoints.

[0.2.0] 2022-11-30
------------------

- **BREAKING** Add high-speed driver support.
  - Remove the `full_speed` module, and move `BusAdapter` into the crate root.
    See the before / after below to update your code.

    ```rust
    // Before:
    use imxrt_usbd::full_speed::BusAdapter;

    // After:
    use imxrt_usbd::BusAdapter;
    ```

  - `BusAdapter::new` produces a high-speed-capable bus. To throttle your USB
    bus to low / full speed, use `BusAdapter::with_speed`, and provide
    `Speed::LowFull`.

- **BREAKING** Users now allocate space for endpoints. For backwards compatibility,
  allocate the maximum amount of endpoints. Supply the endpoint state to your driver's
  constructor, as show by 'new' in the example below.

- **BREAKING** Change the representation of endpoint memory for I/O. Use `EndpointMemory`
  as a replacement for `static mut [u8; N]`. See the before / after in the example below.

  ```rust
  // NEW: allocate space for endpoint state.
  static EP_STATE: imxrt_usbd::EndpointState = imxrt_usbd::EndpointState::max_endpoints();

  // Endpoint memory before:
  // static mut EP_MEMORY: [u8; 2048] = [0; 2048];
  // Endpoint memory after:
  static EP_MEMORY: imxrt_usbd::EndpointMemory<2048> = imxrt_usbd::EndpointMemory::new();

  // ...

  imxrt_usbd::BusAdapter::with_speed(
      UsbPeripherals::usb1(),
      // unsafe { &mut EP_MEMORY }, // Endpoint memory before
      &EP_MEMORY,                   // Endpoint memory after
      &EP_STATE,                    // <-- NEW
      SPEED,
  )
  ```

- **BREAKING** Change the `unsafe trait Peripherals` API. Implementers must now supply the
  addresses of USB register blocks. See the updated documentation for more details.

- **BREAKING** Update to Rust edition 2021.
- Add support for USB general purpose timers (GPT).
- Fix the endpoint initialization routine, which would incorrectly zero the
  other half's endpoint type.
- Fix documentation of `BusAdapter::new`.

[0.1.0] 2021-03-11
------------------

First release

[Unreleased]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.4.0...HEAD
[0.4.2]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/imxrt-rs/imxrt-usbd/tree/v0.1.0
