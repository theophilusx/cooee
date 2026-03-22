PREFIX ?= $(HOME)/.local
BINDIR := $(PREFIX)/bin
SERVICEDIR := $(HOME)/.config/systemd/user
SOUNDDIR := $(HOME)/.config/cooee/sounds

.PHONY: build install uninstall

build:
	cargo build --release

install: build
	install -Dm755 target/release/cooee $(BINDIR)/cooee
	install -Dm644 data/cooee.service $(SERVICEDIR)/cooee.service
	install -Dm644 data/sounds/notify.ogg $(SOUNDDIR)/notify.ogg
	systemctl --user daemon-reload
	systemctl --user enable --now cooee
	@echo "cooee installed and started"

uninstall:
	systemctl --user disable --now cooee || true
	rm -f $(BINDIR)/cooee
	rm -f $(SERVICEDIR)/cooee.service
	systemctl --user daemon-reload
	@echo "cooee uninstalled"
