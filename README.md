# btoprs

A work-in-progress, standard-library-only terminal resource monitor for Linux
and macOS 26 or later. `btoprs` began with btop++ as its UI inspiration but now
evolves independently, choosing behavior and platform integrations on their own
merits. On Linux it reads the relevant procfs and sysfs CPU, memory, network,
process, mount, thermal, and power-supply entries. On macOS it uses native Mach,
libproc, sysctl, IOKit, and SystemConfiguration interfaces. The core macOS
collector supports both Apple silicon and the Intel Macs supported by macOS 26.
Apple GPU and thermal metrics are optional Apple-silicon features; metrics that
the running OS does not expose are hidden.

On Apple silicon, the GPU collector dynamically resolves IOReport and thermal
interfaces and reads AGX registry snapshots. On macOS 26 and 27 it can report
GPU activity, weighted clock, dominant performance state, estimated power,
temperature, core count, resident unified-memory use, and GPU memory-bandwidth
activity from the AGX DCS histogram. Unified memory is
labeled UMA rather than VRAM. AVE encoder and AVD decoder session counts are
shown directly. Estimated media-engine power and DCS read/write traffic are
shown with their units when the current interval produces usable counters; no
utilization percentage is invented from them. Every private metric is
capability-checked at runtime so missing or renamed channels do not prevent
startup.

```console
cargo build --release
target/release/btoprs
```

The terminal UI exits cleanly with `q` or `Ctrl-C`; `Esc` opens the main menu. Run
`target/release/btoprs --help` for the supported command-line options.
See the [project notes](NOTES.md) for platform decisions, known limitations,
and verification guidance.

Install the executable, documentation, themes, and man page under `~/.local`
with:

```console
make install
```

On Linux, this also installs the desktop entry and icons. These desktop files
are skipped on macOS. The platform is detected automatically; packagers can
override it with `INSTALL_PLATFORM`.

Remove the installed files with `make uninstall`.

Use `PREFIX` and `DESTDIR` for another installation root. `cargo install --path
.` remains available when only the `btoprs` executable is wanted.

## License

Licensed under the [Apache License 2.0](LICENSE). btop++ provided the initial UI
inspiration and some derived material; see [NOTICE](NOTICE) for attribution.
