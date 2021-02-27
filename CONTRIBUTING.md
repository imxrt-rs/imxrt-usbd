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

To **build** the package, you must specify

- an `imxrt-ral` feature, *and*
- optionally enable `double-instance`

See the API docs for more information on the `double-instance` feature. Here
are two different examples of how to build the package from the command line:

```
cargo build --features imxrt-ral/imxrt1011
cargo build --features imxrt-ral/imxrt1062 --features double-instance
```

You should be able to just `cargo build` in one of the example packages.
Note that this library can build for your host system, and also for your
embedded ARM target.

If you use **VSCode with rust-analyzer**, you may want to add a
`.vscode/settings.json` configuration when developing for this project.
Otherwise, rust-analyzer won't like the build. The following
settings are what one of the maintainers uses:

```json
{
    "rust-analyzer.cargo.features": [
        "imxrt-ral/imxrt1062",
        "double-instance"
    ]
}
```

To **debug** the library, enable the internal `__log` feature. The feature
enables the library's internal [`log`](https://crates.io/crates/log) hooks.
Then, initialize your logger of choice in your program. See the Teensy 4
examples to see how you might use a UART logger to debug your program.

To test on **hardware**, either

- use an existing example package, or
- contribute a new example package for your system
