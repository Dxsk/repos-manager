#!/usr/bin/env bash
# Transitional stub, kept for a while after 1.0. 0.x (Bash) installs read
# VERSION from this file on GitHub main to show their update banner, and
# git-clone installs execute it after `repos-manager update` pulls.
VERSION="1.0.0"

cat >&2 <<EOF
repos-manager ${VERSION} is a native binary and replaces the 0.x Bash script.
The 0.x updater cannot install it, please reinstall:

  rm -f ~/.local/bin/repos-manager && rm -rf ~/.local/lib/repos-manager
  curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/installers/install.sh | sh

Your config and synced repositories are kept. Full guide:
https://github.com/Dxsk/repos-manager#upgrading-from-0x
EOF
exit 1
