# btop++ to Rust parity audit

Audited against the upstream btop source at commit `d5e5619` (btop
1.4.7). This is a source-to-source audit; screenshots alone are not considered
proof of parity.

## Current status

The dependency-free Rust implementation covers the Linux btop runtime,
collectors, interface, menus, mouse controls, graphs, themes, configuration,
signals and installation layout. The macOS runtime now has native Mach,
libproc, sysctl, getifaddrs, CoreFoundation and IOKit collectors for CPU,
memory/swap, mounted volumes and disk I/O, networking, processes, battery state
and thermal sensors, plus an IOReport Apple Silicon GPU collector with DVFS
clock data. Stable
`--help` and `--default-config` outputs match btop byte-for-byte.

## Remaining parity work

- Platform support: finish macOS CPU/package power and platform-specific option
  lists; port the FreeBSD, OpenBSD and NetBSD collectors.
- GPU edge cases: finish AMD/NVIDIA naming, warning and metric-error behavior;
  verify AMD and Intel metric combinations, multi-GPU layouts and narrow
  branches on real hardware. This machine only provides an RTX A4000, while
  AMD and Intel currently have source and fixture coverage.
- Filesystem edge cases: finish exact device identity and removable/network
  filesystem behavior.
- Narrow memory layout: verify every memory, inline-swap and disk width/height
  combination against btop.
- Presentation details: finish the remaining byte-exact logger messages and
  bold/unbold attributes across every enabled, selected and disabled control.

## Verification gate

- The regular unit, fixture, menu and mouse suite passes on Linux and macOS,
  with an additional opt-in live macOS collector test.
- The opt-in live-NVML test passes on the installed RTX A4000.
- `cargo clippy --all-targets -- -D warnings` and the release build pass.
- Help and default-config output match btop byte-for-byte.
- A staged installation, PTY fatal-signal and UTF-8 checks, and a 120x40
  side-by-side framebuffer comparison have passed.
- The installed executable remains `btoprs`; the crate has no external
  dependencies.
