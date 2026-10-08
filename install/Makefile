# Build-from-source install. Run from the repo root with: make -C install install
PREFIX ?= $(HOME)/.local
ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/..)
BIN := $(ROOT)/target/release/repos-manager
DEST := $(DESTDIR)$(PREFIX)/bin/repos-manager

BASH_COMP := $(HOME)/.local/share/bash-completion/completions/repos-manager
ZSH_COMP := $(HOME)/.local/share/zsh/site-functions/_repos-manager
FISH_COMP := $(HOME)/.config/fish/completions/repos-manager.fish

.PHONY: all build install uninstall completions uninstall-completions

all: build

build:
	cd "$(ROOT)" && cargo build --release --locked

install: build
	install -d "$(DESTDIR)$(PREFIX)/bin"
	install -m 755 "$(BIN)" "$(DEST)"
	@echo "Installed to $(DEST)"

uninstall: uninstall-completions
	rm -f "$(DEST)"
	@echo "Removed $(DEST)"

completions: build
	install -d "$(dir $(BASH_COMP))" "$(dir $(ZSH_COMP))" "$(dir $(FISH_COMP))"
	"$(BIN)" completions bash > "$(BASH_COMP)"
	"$(BIN)" completions zsh > "$(ZSH_COMP)"
	"$(BIN)" completions fish > "$(FISH_COMP)"
	@echo "Completions installed. For zsh, make sure $(dir $(ZSH_COMP)) is in your fpath."

uninstall-completions:
	rm -f "$(BASH_COMP)" "$(ZSH_COMP)" "$(FISH_COMP)"
