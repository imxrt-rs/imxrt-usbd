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

- Add support for USB general purpose timers (GPT).
- Fix the endpoint initialization routine, which would incorrectly zero the
  other half's endpoint type.
- Fix documentation of `BusAdapter::new`.

[0.1.0] 2021-03-11
------------------

First release

[Unreleased]: https://github.com/imxrt-rs/imxrt-usbd/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/imxrt-rs/imxrt-usbd/tree/v0.1.0
