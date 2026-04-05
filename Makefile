PREFIX ?= ~/.local

install:
	@mkdir -p $(PREFIX)/bin $(PREFIX)/lib/repos-manager
	@cp lib/*.sh $(PREFIX)/lib/repos-manager/
	@cp repos-manager.sh $(PREFIX)/bin/repos-manager
	@chmod +x $(PREFIX)/bin/repos-manager
	@sed -i 's|REPOS_MANAGER_LIB:-.*}|REPOS_MANAGER_LIB:-$(PREFIX)/lib/repos-manager}|' $(PREFIX)/bin/repos-manager
	@echo "Installed to $(PREFIX)/bin/repos-manager"

uninstall:
	@rm -f $(PREFIX)/bin/repos-manager
	@rm -rf $(PREFIX)/lib/repos-manager
	@echo "Uninstalled"

lint:
	shellcheck -x repos-manager.sh lib/*.sh sourceme.bash
	zsh -n sourceme.zsh
	fish --no-execute sourceme.fish

test:
	bats tests/*.bats

check: lint test

CURRENT_VERSION := $(shell grep '^VERSION=' repos-manager.sh | cut -d'"' -f2)
MAJOR := $(word 1,$(subst ., ,$(CURRENT_VERSION)))
MINOR := $(word 2,$(subst ., ,$(CURRENT_VERSION)))
PATCH := $(word 3,$(subst ., ,$(CURRENT_VERSION)))

release-patch:
	@$(MAKE) _release NEW_VERSION=$(MAJOR).$(MINOR).$(shell echo $$(($(PATCH)+1)))

release-minor:
	@$(MAKE) _release NEW_VERSION=$(MAJOR).$(shell echo $$(($(MINOR)+1))).0

release-major:
	@$(MAKE) _release NEW_VERSION=$(shell echo $$(($(MAJOR)+1))).0.0

_release:
	@echo "Bumping v$(CURRENT_VERSION) -> v$(NEW_VERSION)..."
	@sed -i 's/^VERSION=".*"/VERSION="$(NEW_VERSION)"/' repos-manager.sh
	@sed -i 's/"version": ".*"/"version": "$(NEW_VERSION)"/' site/src/_data/site.json
	@sed -i 's/repos-manager [0-9]\+\.[0-9]\+\.[0-9]\+/repos-manager $(NEW_VERSION)/' tests/cli.bats
	@git add repos-manager.sh site/src/_data/site.json tests/cli.bats
	@git commit -m "chore: bump version to v$(NEW_VERSION)"
	@git tag "v$(NEW_VERSION)"
	@git push origin main --tags
	@echo "v$(NEW_VERSION) released!"

.PHONY: install uninstall lint test check release-patch release-minor release-major _release
