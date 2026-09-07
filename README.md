# btoprs

A terminal resource monitor inspired by btop++, for Linux and macOS 26 or later.
Keep an eye on CPU, memory, disks, network traffic, and running processes, with
GPU monitoring where supported.

## Install and run

With Rust and Cargo installed:

```console
cargo install btoprs
btoprs
```

Press `Esc` to open the menu and `q` or `Ctrl-C` to quit. Run `btoprs --help`
for command-line options.

## Make it yours

Choose a theme and adjust settings from the menu. All bundled themes are
included in the executable, and btop-compatible `.theme` files are supported.
Add custom themes to `~/.config/btoprs/themes`.

Settings are saved to `~/.config/btoprs/btoprs.conf`. Both paths follow
`$XDG_CONFIG_HOME` when set. Existing btop settings are imported on first use
without changing the original file. Use `btoprs --config <file>` to select a
different configuration file.

See the [project notes](NOTES.md) for platform details and known limitations.

## License

Licensed under the [Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for attribution.
