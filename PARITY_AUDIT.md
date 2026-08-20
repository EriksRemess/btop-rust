# btop++ to Rust parity audit

Audited against `/home/eriks/Development/btop` at commit `d5e5619` (btop
1.4.7). This is a source-to-source audit, not a claim that matching screenshots
prove behavioral parity.

The stable `--help` and `--default-config` outputs match byte-for-byte. The Rust
package has no external crate dependencies and currently implements only the
Linux runtime.

## Runtime and terminal lifecycle

- [ ] `force_utf` and UTF-8 locale detection/failure behavior are ported.
  Debug timings and the configured logger (`btop.cpp`, `btop_log.cpp`) remain.
- [ ] SIGINT, SIGTSTP, SIGCONT, SIGWINCH, SIGUSR1 and SIGUSR2 interrupt the
  input poll and follow normal terminal teardown/reload/redraw paths. Fatal
  signal re-raise/crash reporting remains.
- [ ] Port btop's worker/update scheduling, per-box `no_update` behavior,
  background-update pause behavior, error propagation, and crash cleanup.
- [ ] Match terminal capability and TTY auto-detection, cursor/mouse modes,
  resize/minimum-size recovery, Unicode display width, and sanitizing control
  characters in process commands.
- [ ] Make verbose `--version` report the real compiler version and configure
  command rather than fixed Rust/Cargo labels.

## Configuration and presets

- [ ] Port config validation and warnings, dynamic option choices, read-only
  handling, first-run creation, and the exact rewrite/quoting behavior from
  `btop_config.cpp`.
- [x] Port the ten configurable presets and `disable_presets`, including the
  per-preset graph symbols and layout flags.
- [ ] These accepted options are currently UI-only or otherwise have no runtime
  effect and need their source behavior ported:

  - General: `log_level`.
  - CPU: `show_cpu_watts`.
  - Memory/disk: `zfs_hide_datasets`, `zfs_arc_cached`.
  - Process: `proc_info_smaps`, `keep_dead_proc_usage`.
- [x] Apply `clock_format` with `strftime`, including literal text and all
  supported libc format directives.
- [ ] Port selected-battery behavior and all platform-specific/config-generated
  option lists.

## Input and mouse behavior

- [x] Keep process-list Up/Down independent of `vim_keys`, while mapping vim
  `k` to navigation and uppercase `K` to kill as btop does.
- [ ] F1/F2 and the commonly emitted keyboard/mouse escape sequences are
  decoded. Process scrollbar dragging is ported; the remaining
  terminal-specific sequences still need exact source parity.
- [x] Port process follow (`F`) and disk I/O mode (`i`) as real stateful
  controls.
- [x] Port process filter semantics from `matches_filter`, including POSIX
  extended `!` regex filters and kernel-thread filtering.
- [x] Main/help/options/signal menus, CPU controls, process header/footer,
  process scrollbar and disk/network controls use rendered hitboxes, including
  click-outside deselection.
- [x] Port network `sync`, `auto`, `zero`, previous-interface and next-interface
  title controls, including active bold state and reversible per-interface
  counter offsets.
- [x] Make process-tree markers target the rendered PID instead of calculating
  rows from the CPU boundary when GPU panels are present.

## Menus and dialogs

- [x] Match main-menu keyboard and mouse activation, switch and outside-close
  behavior.
- [ ] Port the complete options editor: exact descriptions, validation,
  dynamic sensor/interface/theme choices, string editing, preset editor,
  category/page geometry, buttons, and mouse regions.
- [x] Match help scrolling and mouse close behavior.
- [x] Port signal confirmation, signal chooser and renice dialogs. Signal and
  renice syscall failures now produce errno-specific result overlays.
- [x] Provide the size-error overlay and no-boxes-shown screen.
- [x] Make Esc open the main menu and dim the underlying UI with
  `inactive_fg`, as btop does before drawing an overlay.

## CPU collection and drawing

- [ ] Sensor discovery/selection, CCD/core mapping and the `cpu_core_map`
  override are ported, including dynamic `provider/label` option choices.
  Critical-temperature limits and every non-k10temp/coretemp edge case remain.
- [ ] Frequency modes (`first`, `range`, `lowest`, `highest`, `average`) are
  ported. Active/offline CPU detection, CPU wattage, and container-engine
  detection remain.
- [ ] Match CPU-name trimming exactly and add the upstream AMD/Intel name
  fixtures.
- [ ] Finish all CPU panel geometry: watts/frequency variants,
  field availability, multi-GPU summaries, battery variants, narrow layouts,
  and exact graph update phasing.

## GPU collection and drawing

- [ ] Finish backend lifecycle/error parity, hotplug/reinitialization, backend
  precedence and transient-error handling for NVML, ROCm SMI, AMD sysfs and
  Intel PMU.
- [ ] Port the complete Intel device-name database and remaining AMD/NVIDIA
  naming/metric edge cases.
- [ ] Match dedicated and inline GPU layout for every supported metric,
  multi-GPU arrangement, history, scaling, power maximum, mirror behavior and
  narrow sizes.
- [ ] Port Apple Silicon GPU collection with the macOS collector.
- [ ] Hardware verification remains limited to NVML on the installed RTX A4000;
  AMD and Intel paths have only source/fixture coverage on this machine.

## Memory and disks

- [ ] Mount filtering (`only_physical`, `use_fstab`, `disks_filter`) and
  privileged/free-space selection are ported. Exact filesystem/device identity
  and removable/network filesystem behavior remain.
- [x] Port disk read/write/activity histories, real IO%, big I/O mode,
  combined/separate graphs, per-device graph speeds, and the `i` control.
- [ ] Port ZFS datasets/pool totals/ARC handling and the related options.
- [ ] Match every memory/swap/disk geometry controlled by `mem_graphs`,
  `mem_below_net`, `show_swap`, `swap_disk`, and panel width/height.

## Network

- [x] Port IPv6 display, counter rollover, running-interface selection and
  btop's five-sample auto-scale hysteresis/rescale behavior.
- [x] Honor manual `net_download`/`net_upload` Mebibit maxima and
  `swap_upload_download`.
- [ ] Match graph history/update phasing and all connected/disconnected/narrow
  layouts.
- [x] Match btop's integer bitrate humanizer and keep statistics inside the
  inner statistics box.

## Processes

- [ ] Details include elapsed time, parent, read/write I/O, status and nice.
  Smaps memory, death time and full detail histories remain.
- [x] Port per-process CPU histories and real five-column CPU graphs.
- [ ] Per-core scaling, kernel-thread filtering and tree aggregation are ported.
  Exact CPU direct/lazy accounting and dead-process retention remain.
- [ ] Tree filtering includes matching rows and their descendants, collapsed
  rows aggregate hidden resources, configured aggregation and auto-collapse
  work, and hidden selections relocate to a surviving parent. Exact prefix
  topology, pause edge cases and every child-toggle transition remain.
- [ ] Follow mode, followed selection styling, process colors and gradients are
  ported. The source-style non-tree `Pid`/`Program`/`Command` columns, selected
  graph background and detail-row text attributes now match; some pause/page
  edge semantics remain.
- [ ] User `+` markers, source-style command clipping/selection and ASCII
  control-character replacement are ported. Exact terminal-column width for
  all combining and double-width Unicode remains.

## Layout, graphs and styling

- [ ] Port `Draw::calcSizes` rather than relying on fixed percentages. This
  includes `proc_left`, `mem_below_net`, `cpu_bottom`, every shown-box
  combination, GPU vectors and exact minimum sizes.
- [ ] Match btop's incremental graph algorithm, resize preservation, graph
  offsets, symbol overrides and redraw/no-update behavior.
- [ ] Complete bold/unbold and other text attributes across every control and
  selected/disabled state. CPU/GPU summary rows, process details, filter editor,
  menus/dialogs and the network header now have source-derived attribute tests.
- [ ] Add deterministic screenshot/glyph/style comparisons at the terminal
  sizes and box combinations used by btop.

## Platforms, packaging and tests

- [ ] Port macOS, FreeBSD, OpenBSD and NetBSD collectors. The Rust collector is
  Linux-specific.
- [x] Add btop-equivalent installation assets and targets: documentation,
  themes, desktop entry, PNG/SVG icons and man page. The installed executable
  remains `btoprs`.
- [ ] The gate covers 91 regular unit/fixture/menu/mouse tests plus one ignored
  live-NVML hardware test, exact
  help/default-config output, Clippy, release build and a staged install. PTY
  SIGINT/UTF-8 runs and a 120x40 side-by-side framebuffer comparison were run
  manually; deterministic checked-in screen comparisons still remain.
