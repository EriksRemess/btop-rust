# btop++ to Rust parity audit

Audited against `/home/eriks/Development/btop` at commit `d5e5619` (btop
1.4.7). This is a source-to-source audit; screenshots alone are not considered
proof of parity.

## Current status

The dependency-free Rust implementation covers the Linux btop runtime,
collectors, interface, menus, mouse controls, graphs, themes, configuration,
signals and installation layout. Stable `--help` and `--default-config` outputs
match btop byte-for-byte.

## Remaining parity work

- Platform support: port the macOS, FreeBSD, OpenBSD and NetBSD collectors,
  their option lists, and the Apple Silicon GPU collector.
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

- 111 regular unit, fixture, menu and mouse tests pass.
- The opt-in live-NVML test passes on the installed RTX A4000.
- `cargo clippy --all-targets -- -D warnings` and the release build pass.
- Help and default-config output match btop byte-for-byte.
- A staged installation, PTY fatal-signal and UTF-8 checks, and a 120x40
  side-by-side framebuffer comparison have passed.
- The installed executable remains `btoprs`; the crate has no external
  dependencies.
