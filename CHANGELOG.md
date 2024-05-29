Changelog
=========

[Unreleased]
------------

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

[Unreleased]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/imxrt-rs/imxrt-usbd/tree/v0.1.0
