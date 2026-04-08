---
title: Getting Started
description: Install repos-manager and sync your first repositories in under 2 minutes.
order: 1
---

## Prerequisites

- `git` and `jq` (required)
- `bash` 4+ (default on all modern Linux distros and macOS)
- `curl` and `yq`, required only if you use the Forgejo / Gitea provider (see [Providers](/docs/providers/))
- At least one provider CLI: `gh`, `glab`, `tea`, `bitbucket`, or `rad`

### Install dependencies by distro

| Distro | Command |
|--------|---------|
| Arch / CachyOS / Manjaro | `sudo pacman -S git jq github-cli` |
| Debian / Ubuntu / Mint | `sudo apt install git jq gh` |
| Fedora / RHEL / CentOS | `sudo dnf install git jq gh` |
| openSUSE | `sudo zypper install git jq gh` |
| Alpine | `apk add git jq github-cli` |
| Void Linux | `sudo xbps-install git jq github-cli` |
| Gentoo | `emerge dev-vcs/git app-misc/jq dev-util/github-cli` |
| macOS (Homebrew) | `brew install git jq gh` |

## Installation

### Make (recommended)

Works on any Linux distro or macOS with `git`, `make`, and `bash`:

```bash
git clone git@github.com:Dxsk/repos-manager.git
cd repos-manager
make install
```

This installs to `~/.local/bin/repos-manager`. Change the prefix with:

```bash
make install PREFIX=/usr/local
```

Run lint and tests:

```bash
make check   # lint + tests
make lint    # shellcheck + zsh/fish syntax
make test    # bats tests
```

### Manual

Source the file for your shell:

```bash
# bash
source sourceme.bash

# zsh
source sourceme.zsh

# fish
source sourceme.fish
```

### Nix

```bash
nix run github:Dxsk/repos-manager -- github sync
```

## First sync

```bash
# Authenticate with your providers
repos-manager login

# Sync all repos
repos-manager sync --all
```

After syncing, your repos are organized as:

```bash
~/Documents/
  github.com/
    your-user/
      repo-1/
      repo-2/
    your-org/
      project/
  gitlab.com/
    ...
```

## Verify

```bash
# Check status of all repos
repos-manager status
```
