imxrt-usbd
=========

[![crates.io][]][1] [![docs.rs]][2]

  [crates.io]: https://img.shields.io/crates/v/imxrt-usbd
  [1]: https://crates.io/crates/imxrt-usbd
  [docs.rs]: https://docs.rs/imxrt-usbd/badge.svg
  [2]: https://docs.rs/imxrt-usbd/

**[API Docs (main branch)][main-api-docs]**

A USB driver for i.MX RT processors. `imxrt-usbd` provides a [`usb-device`]
USB bus implementation, allowing you to add USB device features to your
embedded Rust program. It should support all i.MX RT microcontrollers.

[`imxrt-ral`]: https://crates.io/crates/imxrt-ral
[main-api-docs]: https://imxrt-rs.github.io/imxrt-usbd/
[`usb-device`]: https://crates.io/crates/usb-device

See the API docs for usage, features, and examples. To try examples on actual
hardware, see the [`examples` directory](./examples).


License
-------

Licensed under either of

- [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0) ([LICENSE-APACHE](./LICENSE-APACHE))
- [MIT License](http://opensource.org/licenses/MIT) ([LICENSE-MIT](./LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
