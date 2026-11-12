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

### Snow Leopard (this branch)

The Intel macOS backend targets the 10.6 SDK. Mach CPU/VM counters and native
sysctl/libproc supply system and accessible-process data. The VM layout is the
96-byte Snow Leopard structure; inactive pages are treated as reclaimable.
Compressed-memory and modern per-process disk-I/O APIs are not used.

GPU collection is a separate compact legacy backend. It reads only Radeon
`VRAM,totalsize` and `vramFreeBytes` and Intel SMC `TG0D`/`TC0D` temperatures.
There are no IOReport, HID, AGX, NVML, ROCm, Vulkan or Linux GPU probes in the
macOS binary. SMC commands are limited to key metadata and byte reads.

Disk byte counters are found on the physical IOBlockStorageDriver ancestor.
Only one mounted volume per physical drive receives these rates. No disk busy
percentage is inferred from overlapping read/write service times. The fixed
`hw.cpufrequency` value is nominal frequency, not a measured live P-state.

Cross-build and native test instructions are in [README.md](README.md).
Rust's compiler warning about its official 10.12 minimum is expected; linking
and native runtime checks are both required to establish compatibility.

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

On the iMac, run the cross-built test executable serially:

```console
./btoprs-tests --test-threads=1
./btoprs-tests collect::macos::tests::live_collector_returns_core_system_data --ignored --nocapture --test-threads=1
./btoprs-tests live_refresh_releases_host_ports --ignored --test-threads=1
```

The build script records the test executable path in
`target/snow-leopard/test-executables.json`. Native UI validation uses
`scripts/snow-leopard/ui-probe.m` with an isolated Snow Ghostty process, config
and temporary home. It checks monitor/options/help/filter rendering, fullscreen
resize in both directions, normal quit and restoration of the original PTY
attributes. Do not direct test key events at a user's existing terminal.

## Snow Leopard validation, 2026-11-12

- Cross-built with Rust 1.99.1, rebuilt std/pthread TLS, Core 2 and the 10.6 SDK.
- Native suite: 187 regular tests plus the two separately run live collector and
  Mach host-port reference checks. Linux suite: 199 tests. Both target Clippy
  checks passed with warnings denied, apart from Rust's expected OS-minimum
  diagnostic (not a source-code lint).
- Live reads verified two CPU cores, 4 GiB RAM, swap, physical-drive byte counters,
  en0 traffic/addressing, accessible processes, CPU/GPU temperatures and 128 MiB
  Radeon VRAM with bounded current use.
- Visually inspected the real Snow Ghostty monitor, help and settings captures.
  Filtering, fullscreen/restore resizing and normal quit passed. Original PTY
  attributes were restored, excluding only transient Darwin PENDIN state.

These checks establish this specific application/toolchain/hardware combination,
not general Rust support for macOS 10.6 or untested GPU drivers.

### Sudo configuration regression

Atomic config replacement preserves the existing file's UID/GID as well as its
mode. If ownership cannot be retained, saving fails without replacing the old
file. This prevents a sudo-run monitor from leaving a root-owned replacement in
the invoking user's configuration directory.

An additional ignored test was run with sudo on the physical iMac. It creates
an isolated mode-0700 temporary fixture, changes its config owner to SUDO_UID/
SUDO_GID, saves through the actual Config API and verifies ownership, mode and
new contents before cleanup:

```console
sudo ./btoprs-tests config::tests::elevated_save_preserves_original_owner --ignored --exact --test-threads=1
```

The user's already affected file was repaired by changing only its ownership;
its saved contents were retained.
