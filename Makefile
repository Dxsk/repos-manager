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

.PHONY: install uninstall
