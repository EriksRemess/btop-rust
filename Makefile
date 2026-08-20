PREFIX ?= $(HOME)/.local
DESTDIR ?=

.PHONY: all test install uninstall

all:
	cargo build --release

test:
	cargo test

install: all
	@echo "Installing binary to: $(DESTDIR)$(PREFIX)/bin/btoprs"
	install -Dm755 target/release/btoprs "$(DESTDIR)$(PREFIX)/bin/btoprs"
	@echo "Installing doc to: $(DESTDIR)$(PREFIX)/share/doc/btoprs"
	install -Dm644 README.md "$(DESTDIR)$(PREFIX)/share/doc/btoprs/README.md"
	install -Dm644 PARITY_AUDIT.md "$(DESTDIR)$(PREFIX)/share/doc/btoprs/PARITY_AUDIT.md"
	@echo "Installing themes to: $(DESTDIR)$(PREFIX)/share/btop/themes"
	install -d "$(DESTDIR)$(PREFIX)/share/btop/themes"
	install -m644 themes/*.theme "$(DESTDIR)$(PREFIX)/share/btop/themes/"
	@echo "Installing desktop entry to: $(DESTDIR)$(PREFIX)/share/applications/btoprs.desktop"
	install -Dm644 assets/btoprs.desktop "$(DESTDIR)$(PREFIX)/share/applications/btoprs.desktop"
	@echo "Installing PNG icon to: $(DESTDIR)$(PREFIX)/share/icons/hicolor/48x48/apps/btoprs.png"
	install -Dm644 assets/btoprs.png "$(DESTDIR)$(PREFIX)/share/icons/hicolor/48x48/apps/btoprs.png"
	@echo "Installing SVG icon to: $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/btoprs.svg"
	install -Dm644 assets/btoprs.svg "$(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/btoprs.svg"
	@echo "Installing man page to: $(DESTDIR)$(PREFIX)/share/man/man1/btoprs.1"
	install -Dm644 assets/btoprs.1 "$(DESTDIR)$(PREFIX)/share/man/man1/btoprs.1"

uninstall:
	@echo "Removing btoprs from: $(DESTDIR)$(PREFIX)"
	rm -f "$(DESTDIR)$(PREFIX)/bin/btoprs"
	rm -f "$(DESTDIR)$(PREFIX)/share/doc/btoprs/README.md"
	rm -f "$(DESTDIR)$(PREFIX)/share/doc/btoprs/PARITY_AUDIT.md"
	@for theme in themes/*.theme; do \
		rm -f "$(DESTDIR)$(PREFIX)/share/btop/themes/$${theme##*/}"; \
	done
	rm -f "$(DESTDIR)$(PREFIX)/share/applications/btoprs.desktop"
	rm -f "$(DESTDIR)$(PREFIX)/share/icons/hicolor/48x48/apps/btoprs.png"
	rm -f "$(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/btoprs.svg"
	rm -f "$(DESTDIR)$(PREFIX)/share/man/man1/btoprs.1"
	@rmdir "$(DESTDIR)$(PREFIX)/share/doc/btoprs" 2>/dev/null || true
	@rmdir "$(DESTDIR)$(PREFIX)/share/btop/themes" 2>/dev/null || true
	@rmdir "$(DESTDIR)$(PREFIX)/share/btop" 2>/dev/null || true
