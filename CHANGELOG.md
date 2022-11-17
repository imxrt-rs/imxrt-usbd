Changelog
=========

[Unreleased]
------------

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

- Add support for USB general purpose timers (GPT).
- Fix the endpoint initialization routine, which would incorrectly zero the
  other half's endpoint type.
- Fix documentation of `BusAdapter::new`.

[0.1.0] 2021-03-11
------------------

First release

[Unreleased]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/imxrt-rs/imxrt-usbd/tree/v0.1.0
