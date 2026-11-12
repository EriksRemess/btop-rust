# btoprs for Snow Leopard

An Intel/Core 2 port of btop-rust for Mac OS X 10.6.8. This branch uses native
Mach, sysctl, libproc and IOKit interfaces available on the old iMac.

It provides aggregate/per-core CPU load, load averages and uptime, nominal CPU
frequency, CPU diode temperature, RAM and swap, mounted-disk capacity and
whole-drive read/write rates, network traffic and addresses, and accessible
processes. The legacy Radeon panel shows driver-reported VRAM use and SMC GPU
temperature. Unsupported metrics and irrelevant settings are omitted.

## Build on Linux

Use Rust **1.99.1** with `rust-src`, Clang, Python 3, a MacOSX10.6 SDK and the
compatible OSXCross stable linker:

```sh
python3 scripts/snow-leopard/build.py \
  --sdk /path/to/MacOSX10.6.sdk \
  --tools /path/to/osxcross/target/bin --tests
```

The executable is `target/snow-leopard/cargo/x86_64-apple-darwin/release/btoprs`.
Copy it to `~/.local/bin/btoprs` on the Mac. No Rust installation, MacPorts
runtime libraries or administrator access is needed on the target. Running
through sudo reveals additional protected processes; saving an existing config
preserves its original owner, group and permissions.

This is an application-specific legacy port, outside Rust's official macOS
minimum. The build reconstructs std with pthread-key TLS, supplies the small
compatibility functions in `scripts/snow-leopard/compat.c`, and verifies 10.6
load commands and `LC_UNIXTHREAD`. Do not substitute a modern linker solely
because it produces a successful build.

## Use

Run `btoprs` in Snow Ghostty or another UTF-8 terminal. `q` quits, `o` opens
settings, `h` opens help and `f` filters processes. Configuration lives at
`~/.config/btoprs/btoprs.conf`; `--config FILE` selects another file. All themes
remain bundled; an example Catppuccin Mocha config is in
[scripts/snow-leopard/btoprs.conf](scripts/snow-leopard/btoprs.conf).

Snow Leopard's unprivileged libproc lists accessible processes, normally those
owned by the current user. Protected processes may be absent. Per-process disk
I/O, per-core temperature, live CPU P-states, CPU/GPU power, GPU utilization,
GPU clocks, PCIe and media-engine telemetry are unavailable. Disk rates describe
the physical drive and are assigned to only one mounted volume per drive; the
driver's cumulative service times are not presented as disk busy percentage.

See [project notes](NOTES.md) for validation and inherited platform details.
The original `main` branch remains the general Linux/modern macOS/Windows app.

The `--tests` build also produces `target/snow-leopard/ui-probe`. Copy that,
its test executable and `scripts/snow-leopard/{test-target.py,run-monitor.py,btoprs.conf}`
to the Mac, then run with MacPorts Python 3.13:

```sh
python3.13 test-target.py --binary ~/.local/bin/btoprs \
  --tests ./btoprs-tests --ui-probe ./ui-probe
```

This opens an isolated Snow Ghostty window and exercises settings, help,
filtering, fullscreen resizing, quit and PTY restoration. Logs and captures stay
in its private temporary directory. Cross-target linting uses the build script's
`--clippy` flag.

## License

[Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for attribution.
