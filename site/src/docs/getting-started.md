---
title: Getting Started
description: Install repos-manager and sync your first repositories in under 2 minutes.
order: 1
---

## Prerequisites

repos-manager is a single native binary for Linux (x86_64, aarch64), macOS (x86_64, aarch64) and Windows (x86_64). At runtime it only needs:

- `git`
- The CLI of each provider you want to sync: `gh`, `glab`, `tea`, `bitbucket` or `rad` (see the Providers page)

### Install git and the GitHub CLI

| Platform | Command |
|----------|---------|
| Arch / CachyOS / Manjaro | `sudo pacman -S git github-cli` |
| Debian / Ubuntu / Mint | `sudo apt install git gh` |
| Fedora / RHEL / CentOS | `sudo dnf install git gh` |
| openSUSE | `sudo zypper install git gh` |
| Alpine | `apk add git github-cli` |
| Void Linux | `sudo xbps-install git github-cli` |
| Gentoo | `emerge dev-vcs/git dev-util/github-cli` |
| macOS (Homebrew) | `brew install git gh` |
| Windows (winget) | `winget install Git.Git GitHub.cli` |

## Installation

Releases are published on [GitHub Releases](https://github.com/Dxsk/repos-manager/releases) and on the [forge](https://forge.infrasouveraine.fr/dxsk/repos-manager/releases). Every release ships `SHA256SUMS`, which all install methods below verify.

### Linux and macOS

```bash
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.sh | sh
```

The script picks the archive for your OS and CPU, checks it against `SHA256SUMS` and installs the binary into `~/.local/bin`. Options go after `sh -s --`:

| Option | Default | Description |
|--------|---------|-------------|
| `--version vX.Y.Z` | latest | Release to install |
| `--prefix DIR` | `~/.local` | Installs into `DIR/bin` |
| `--source github\|forge` | `github` | Where to download from |
| `--uninstall` | | Remove the installed binary |

```bash
# Pin a version, system-wide
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.sh | sudo sh -s -- --version v1.0.0 --prefix /usr/local

# Use the forge as source (macOS builds are only published on GitHub)
curl -fsSL https://forge.infrasouveraine.fr/dxsk/repos-manager/raw/branch/main/install/install.sh | sh -s -- --source forge
```

### Windows

With PowerShell:

```powershell
irm https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.ps1 | iex
```

It installs `repos-manager.exe` into `%LOCALAPPDATA%\Programs\repos-manager` and adds that folder to your user `PATH`. Open a new terminal afterwards. To pin a version, set `$env:REPOS_MANAGER_VERSION = "v1.0.0"` first. A saved copy of the script also accepts `-Version`, `-Source forge` and `-Uninstall`.

Prefer an installer? Download `repos-manager-windows-x86_64-setup.exe` from the [latest release](https://github.com/Dxsk/repos-manager/releases/latest). It installs for the current user without admin rights, adds the folder to `PATH` and registers an uninstaller in *Apps and features*. Silent install: `repos-manager-windows-x86_64-setup.exe /S`.

### From source

Requires a recent stable [Rust toolchain](https://rustup.rs) and `make`:

```bash
git clone https://forge.infrasouveraine.fr/dxsk/repos-manager.git
cd repos-manager
make -C install install        # cargo build --release, then copy to ~/.local/bin
make -C install completions    # bash, zsh and fish completions
```

Change the prefix with `make -C install install PREFIX=/usr/local`. Remove everything with `make -C install uninstall`.

Or install straight from the repository with cargo:

```bash
cargo install --git https://forge.infrasouveraine.fr/dxsk/repos-manager.git
```

### Shell completions

```bash
repos-manager completions bash > ~/.local/share/bash-completion/completions/repos-manager
repos-manager completions zsh  > ~/.local/share/zsh/site-functions/_repos-manager
repos-manager completions fish > ~/.config/fish/completions/repos-manager.fish
```

```powershell
repos-manager completions powershell | Out-String | Invoke-Expression
```

`elvish` is also supported.

## First sync

```bash
# Optional: write the default config file
repos-manager init

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
