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

Per-process GPU telemetry currently supports NVIDIA through optional NVML process
queries. Graphics and compute entries are merged by PID and GPU instance;
duplicate reports are counted once. The process column sums reported NVIDIA
devices only, and remains unavailable when a device query is incomplete or a
memory value is unavailable. Device totals can include allocations not
attributable to a process. PID start times are rechecked after GPU collection.
GPU utilization uses the latest per-process SM sample from the past second;
idle processes show zero, unsupported queries remain unavailable. Utilization
is summed across reported NVIDIA devices and can exceed 100% with multiple GPUs.
An initial process-query probe enables the feature even when the GPU is idle.
Failed probes retry every ten seconds; after support is established, process
collection runs only when the panel is requested or process details need it.
Transient sample failures remain unavailable readings without hiding the feature.
Without a supported backend, the process toggle, panel, options, and GPU detail
row are hidden; saved GPU process settings are preserved. AMD/Intel per-process
data is not yet implemented, so this feature is also hidden on Windows and macOS.

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

### Windows

Windows support uses the Win32 console in virtual-terminal mode. System data
comes from native NT, Kernel32, PSAPI, Tool Help, IP Helper, registry, and power
management APIs. The initial backend includes aggregate and per-logical-CPU
usage and current frequency, memory and system cache, actual page-file use,
logical-drive capacity, default-route network selection, IPv4 and traffic
counters, battery state, disk throughput and activity, and process CPU, memory,
command line, account, thread, priority, and I/O data. Process termination and
priority controls use native process APIs.

NVIDIA GPU telemetry is available through the driver-supplied NVML library.
AMD and Intel GPU telemetry, CPU temperature and power, IPv6 display, and
per-process scheduler state are not yet implemented on Windows. Unsupported
values are omitted or left unavailable rather than synthesized.

## Known limitations

- FreeBSD, OpenBSD, and NetBSD collectors have not been ported.
- Windows AMD/Intel GPU and CPU hardware-sensor telemetry is not implemented
  yet.
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

On an Apple-silicon Mac, run the live macOS checks separately from the NVIDIA
hardware test. Run them serially because the host-port leak check measures
process-wide Mach reference counts:

```console
cargo test macos:: -- --ignored --test-threads=1
cargo clippy --all-targets --target x86_64-apple-darwin -- -D warnings
cargo build --release --target x86_64-apple-darwin
```

The Intel checks require the `x86_64-apple-darwin` Rust target. Cross-compilation
checks the build and native bindings; live Intel telemetry still needs Intel
hardware. Apple live tests require the telemetry capabilities asserted by each
test and are not portability checks for every SoC or macOS version.

CLI and configuration changes should preserve deliberate backward compatibility
or include a clear migration path. Installation should be checked with both the
native platform and a staged `DESTDIR`.

The elevated config-save regression is ignored by default because it verifies
replacement of a config owned by the invoking user while the saver is root.
It runs in an exclusive mode-0700 temporary fixture and uses SUDO_UID/SUDO_GID.
Build as your normal user, then invoke the test binary printed by Cargo for
this single privileged case:

```console
cargo test --no-run
sudo /path/to/btoprs-test-binary config::tests::elevated_save_preserves_original_owner --ignored --exact --test-threads=1
```
