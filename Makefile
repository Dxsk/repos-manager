PREFIX ?= $(HOME)/.local
BIN    := target/release/repos-manager

BASH_COMP := $(HOME)/.local/share/bash-completion/completions/repos-manager
ZSH_COMP  := $(HOME)/.local/share/zsh/site-functions/_repos-manager
FISH_COMP := $(HOME)/.config/fish/completions/repos-manager.fish

build:
	@command -v cargo >/dev/null || { echo "cargo not found: install Rust (https://rustup.rs) or use installers/install.sh"; exit 1; }
	cargo build --release --locked

test:
	cargo test --locked

# Needs cargo-llvm-cov: cargo install cargo-llvm-cov
coverage:
	cargo llvm-cov --locked --summary-only

lint:
	cargo fmt --check
	cargo clippy --all-targets --locked -- -D warnings

install: build
	install -Dm755 $(BIN) $(DESTDIR)$(PREFIX)/bin/repos-manager

uninstall: uninstall-completions
	rm -f $(DESTDIR)$(PREFIX)/bin/repos-manager

completions: build
	install -d $(dir $(BASH_COMP)) $(dir $(ZSH_COMP)) $(dir $(FISH_COMP))
	$(BIN) completions bash > $(BASH_COMP)
	$(BIN) completions zsh > $(ZSH_COMP)
	$(BIN) completions fish > $(FISH_COMP)
	@echo "For zsh, make sure $(dir $(ZSH_COMP)) is in your fpath."

uninstall-completions:
	rm -f $(BASH_COMP) $(ZSH_COMP) $(FISH_COMP)

clean:
	cargo clean

# make release V=1.1.0
# Bumps the version, dates the "Unreleased" changelog section, commits and
# tags. Pushing the tag to Forgejo (and the mirror) triggers the releases.
release:
	@test -n "$(V)" || { echo "usage: make release V=x.y.z"; exit 1; }
	@git diff --quiet && git diff --cached --quiet || { echo "commit your changes first"; exit 1; }
	@grep -q '^## Unreleased' CHANGELOG.md || { echo "CHANGELOG.md has no '## Unreleased' section"; exit 1; }
	sed -i.bak 's/^version = ".*"/version = "$(V)"/' Cargo.toml && rm Cargo.toml.bak
	sed -i.bak 's/"version": ".*"/"version": "$(V)"/' site/src/_data/site.json && rm site/src/_data/site.json.bak
	sed -i.bak 's/^VERSION=".*"/VERSION="$(V)"/' repos-manager.sh && rm repos-manager.sh.bak
	sed -i.bak "s/^## Unreleased/## $(V) ($$(date +%F))/" CHANGELOG.md && rm CHANGELOG.md.bak
	cargo update --workspace --offline
	git commit -m "chore: release v$(V)" Cargo.toml Cargo.lock CHANGELOG.md site/src/_data/site.json repos-manager.sh
	git tag -a "v$(V)" -m "v$(V)"
	@echo "Now run: git push --follow-tags"

.PHONY: build test coverage lint install uninstall completions uninstall-completions clean release
