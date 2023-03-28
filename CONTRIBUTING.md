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
build for your host system, and also for your embedded ARM target. To build for
an embedded target, include a `--target` flag, like

```
cargo build --target thumbv7em-none-eabihf
```

To run **unit tests**, change `cargo build` to `cargo test` in the above
examples. These tests run on your host system.

To **debug** the library, enable the internal `__log` feature. The feature
enables the library's internal [`log`](https://crates.io/crates/log) hooks.
Then, initialize your logger of choice in your program. You may also need to
configure the maximum log level as a feature on the `log` crate.

To test on **hardware**, refer to the hardware examples maintained in the
[imxrt-hal project](https://github.com/imxrt-rs/imxrt-hal). Those examples
work on multiple development boards. We welcome new USB example contributions
there. If the imxrt-hal project does not support your development board, open
an issue in the imxrt-hal issue tracker.

If you're testing imxrt-usbd changes with the imxrt-hal examples, you'll need
a way to integrate your patches into that project's build. [Consider using
patches to override dependencies][patch].

[patch]: https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html

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
