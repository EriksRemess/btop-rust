# Project notes

## Scope

`btoprs` is an independent, standard-library-only terminal resource monitor.
btop++ 1.4.7 was the initial UI reference, not an ongoing compatibility target.
New behavior is chosen for clarity, usefulness, robustness, and the capabilities
of each supported platform. Existing behavior can be retained when it remains a
good fit, but upstream differences do not by themselves constitute bugs.

Platform behavior is verified against native documentation, system headers,
and the interfaces exposed by the running OS. This is especially important on
macOS, where Linux-oriented conventions and another monitor's implementation
are not authoritative.

The installed executable and Cargo package are named `btoprs`. The project has
no runtime crate dependencies.

## Platform notes

### Linux

Linux data comes from procfs, sysfs, perf events, and dynamically loaded GPU
interfaces. Desktop entries and icons are Linux installation artifacts and are
not installed on macOS.

### macOS

The general macOS collectors use Mach, libproc, sysctl, getifaddrs,
SystemConfiguration, CoreFoundation, and IOKit. Public interfaces are preferred
where they provide the metric. Apple-silicon CPU and GPU details additionally
use private IOReport interfaces resolved at runtime, plus power-manager and AGX
registry data, HID thermal sensors, and an SMC fallback. The CPU clock is
residency-weighted from per-core performance states and the matching DVFS
frequency tables and is shown per core when available; the legacy
`hw.cpufrequency` sysctl remains the Intel and fallback path. Private metrics
are capability-checked so missing or renamed channels do not prevent startup.

The Apple GPU panel can show interval activity, residency-weighted frequency,
the dominant P-state, estimated power, temperature, hardware core count,
resident unified-memory use, and normalized AGX memory-bandwidth activity.
Unified memory is labeled UMA because it is shared system memory, not dedicated
VRAM.

AVE encoder and AVD decoder values are session counts, not utilization
percentages. Media-engine power and traffic are displayed only when the current
system supplies a usable value with a meaningful unit. Some macOS 27 systems,
including the tested A18 Pro machine, advertise AVE/VDEC energy channels but
leave them frozen at zero; those unavailable power readings are hidden. The UI
does not infer media utilization, memory clock, PCIe throughput, or dedicated
VRAM values from unrelated counters.

## Known limitations

- FreeBSD, OpenBSD, and NetBSD collectors have not been ported.
- AMD and Intel GPU combinations need broader verification on physical
  hardware; NVIDIA has live coverage on an RTX A4000.
- Device identity and removable or network filesystem edge cases need more
  cross-platform fixtures.
- Extremely constrained terminal layouts still need broader testing.
- Private Apple telemetry can change between OS or SoC generations and must
  continue to degrade by hiding unsupported metrics.

## Verification

The regular suite covers collectors, configuration, menus, mouse controls,
rendering, responsive layouts, and terminal input. Additional ignored tests can
exercise live macOS and NVIDIA APIs on suitable hosts. Before release, run:

```console
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
```

CLI and configuration changes should preserve deliberate backward compatibility
or include a clear migration path. Installation should be checked with both the
native platform and a staged `DESTDIR`.
