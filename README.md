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

Linux GPU naming does not require sudo or membership in a graphics-device
group. Driver-provided names are preferred when accessible; AMD devices fall
back to the installed `pci.ids` hardware database when driver queries are
unavailable. Readable sysfs telemetry remains usable without DRM device access.
When Vulkan is accessible, `--diagnostics` also reports driver details, GPU type,
API version, and memory heap sizes and access properties. The AMD sysfs GPU
panel shows a compact driver/API footer. Vulkan heap sizes describe memory
layout; live usage continues to come from the system collectors.

On Apple silicon, the collectors dynamically resolve IOReport and thermal
interfaces and read power-manager and AGX registry snapshots. CPU frequency is
derived from per-core performance-state residency and the SoC's frequency
tables when the legacy CPU-frequency sysctl is unavailable. Available live
clocks are shown beside the individual CPU cores. On macOS 26 and 27 the GPU
collector can report activity, weighted
clock, dominant performance state, estimated power, temperature, core count,
resident unified-memory use, and GPU memory-bandwidth activity from the AGX DCS
histogram. Unified memory is
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

`btoprs` supports btop-compatible `.theme` files. All bundled themes are embedded
in the executable, including Cargo installations. On startup, missing themes
are installed into `$XDG_CONFIG_HOME/btoprs/themes` (normally
`~/.config/btoprs/themes`). Themes already present in any search directory are
left in place, so your edits survive upgrades. Deleted bundled themes are restored
on the next startup if no other copy exists.

Readable theme files take precedence over embedded copies. If a bundled theme's
file cannot be read, or the theme directory cannot be written, the embedded copy
remains available directly from the executable.
Legacy btop theme directories are also searched for compatibility.

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
.` installs the executable with all bundled themes embedded; desktop files and
the man page are installed by `make install`.

## License

Licensed under the [Apache License 2.0](LICENSE). btop++ provided the initial UI
inspiration and some derived material; see [NOTICE](NOTICE) for attribution.
