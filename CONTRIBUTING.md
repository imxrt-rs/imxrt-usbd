Contributing
============

Thanks for contributing! Please open an issue when

- you find a bug in the crate
- you have an idea for a feature
- something isn't clear in our documentation

The rest of this guide provides quick tips for working with these packages.
Before you get started, make sure that you have all the build dependencies.
We need whatever is required for the [`imxrt-ral`] package. See the `imxrt-ral`
contributing documentation for more information.

[`imxrt-ral`]: https://github.com/imxrt-rs/imxrt-ral

Development
-----------

If you'd like to work with this code, read on for development requirements
and tips.

To **build** the package, just use `cargo build`. You should be able to just
`cargo build` in one of the example packages. Note that this library can
build for your host system, and also for your embedded ARM target.

To run **unit tests**, change `cargo build` to `cargo test` in the above
examples. Rustdoc tests are only guaranteed to compile for
`imxrt-ral/imxrt1062`. Library tests will work for all systems.

To **debug** the library, enable the internal `__log` feature. The feature
enables the library's internal [`log`](https://crates.io/crates/log) hooks.
Then, initialize your logger of choice in your program. See the Teensy 4
examples to see how you might use a UART logger to debug your program.

To test on **hardware**, either

- use an existing example package, or
- contribute a new example package for your system

To **test `usb-device` compatibility**, use [the `usb-device` test class][test-class]
program in each example.

1. Build and flash the `test_class` example onto your hardware. See your
   example's documentation for build and flash instructions. Keep your board
   connected to your host.
2. Clone and enter [the `usb-device` repository][usb-device-repo]

    ```
    git clone https://github.com/mvirkkunen/usb-device.git && cd usb-device
    ```
3. Run `cargo test` in the `usb-device` directory.

[test-class]: https://docs.rs/usb-device/0.2.7/usb_device/test_class/index.html
[usb-device-repo]: https://github.com/mvirkkunen/usb-device

For **design** information, see the API docs. Most modules include a high-level
blurb that talks about what's going on. There are also public-facing design
documentation in some modules.

If you'd like **references**, see

- this [application note][an3631]. Although the AN is for a different
  NXP processor, the USB driver design is the same.
- the i.MX RT reference manuals, available from NXP. Go
  [here][imx-rt-series], and select your processor. Then, go to
  "Documentation," and scroll down to "Reference Manual." You'll need a free
  NXP account to access the reference manuals.

[an3631]: https://www.nxp.com/docs/en/application-note/AN3631.pdf
[imx-rt-series]: https://www.nxp.com/products/processors-and-microcontrollers/arm-microcontrollers/i-mx-rt-crossover-mcus:IMX-RT-SERIES
