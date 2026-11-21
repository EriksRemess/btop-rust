# btoprs

A terminal resource monitor inspired by btop++, for Linux, macOS 26 or later,
and Windows 10 or later.
Keep an eye on CPU, memory, disks, network traffic, and running processes, with
GPU monitoring where supported.

## Install and run

With Rust 1.100 or later and Cargo installed:

```console
cargo install btoprs
btoprs
```

Press `Esc` to open the menu and `q` or `Ctrl-C` to quit. Run `btoprs --help`
for command-line options.

On Linux with an NVIDIA GPU, press `v` or click `gpu View` in the process
footer to show a separate GPU processes panel. It displays GPU utilization,
GPU memory, CPU utilization, and host RAM, with device IDs and VRAM percentage
when space permits. Add `gpu_proc` to `shown_boxes` or a preset to show it on startup.

Click a process panel or press Tab to choose where keyboard controls apply.
Each panel has independent sorting, filtering, selection, scrolling, and pause.
GPU tree view keeps ancestors for context. Enter opens details inside the selected
panel, including per-device GPU readings when space permits. `—` means unavailable.
GPU panel settings are in the Processes options menu. These controls and the
panel appear only when a GPU backend successfully supports process queries.

## Make it yours

Choose a theme and adjust settings from the menu. All bundled themes are
included in the executable, and btop-compatible `.theme` files are supported.
Add custom themes to `~/.config/btoprs/themes`. On Windows, themes are stored
under `%APPDATA%\btoprs\themes`.

Settings are saved to `~/.config/btoprs/btoprs.conf`. Both paths follow
`$XDG_CONFIG_HOME` when set. Existing btop settings are imported on first use
without changing the original file. Use `btoprs --config <file>` to select a
different configuration file. On Windows, the default settings file is
`%APPDATA%\btoprs\btoprs.conf`.

See the [project notes](NOTES.md) for platform details and known limitations.

## License

Licensed under the [Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for attribution.
