# btop-rust

A work-in-progress, standard-library-only Rust port of btop++ 1.4.7. The current
implementation targets Linux and reads the same kernel interfaces as btop++:
the relevant procfs CPU, memory, network, process, and mount entries, plus the
sysfs thermal and power-supply entries.

```console
cargo build --release
target/release/btoprs
```

The terminal UI exits cleanly with `q` or `Ctrl-C`; `Esc` opens the main menu. Run
`target/release/btoprs --help` for the supported btop++ command-line options.
See `PARITY_AUDIT.md` for the exact parity status.

Install the executable, documentation, themes, desktop entry, icons and man page
under `~/.local` with:

```console
make install
```

Remove the installed files with `make uninstall`.

Use `PREFIX` and `DESTDIR` for another installation root. `cargo install --path
.` remains available when only the `btoprs` executable is wanted.

## License

Licensed under the [Apache License 2.0](LICENSE). This port is based on btop++;
see [NOTICE](NOTICE) for upstream attribution.
