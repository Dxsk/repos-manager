---
title: Getting Started
description: "Install repos-manager on Linux, macOS or Windows (script, AUR, cargo, installer), log in to your forges and sync your first repositories."
order: 1
---

## Prerequisites

repos-manager is a single native binary for Linux (x86_64, aarch64), macOS (x86_64, aarch64) and Windows (x86_64, aarch64). At runtime it only needs:

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

Releases are published on [GitHub Releases](https://github.com/Dxsk/repos-manager/releases), which has every platform, and on the [forge](https://forge.infrasouveraine.fr/dxsk/repos-manager/releases), which carries the Linux and Windows x86_64 builds. Every release ships `SHA256SUMS` (archives and installers) and `SHA256SUMS-binaries` (the binary inside each archive). The install scripts verify downloads against `SHA256SUMS`.

### Package managers

| Where | Package | Command |
|-------|---------|---------|
| [crates.io](https://crates.io/crates/repos-manager) | `repos-manager` | `cargo install repos-manager` |
| [AUR](https://aur.archlinux.org/packages/repos-manager-bin) | `repos-manager-bin` (prebuilt) | `yay -S repos-manager-bin` |
| [AUR](https://aur.archlinux.org/packages/repos-manager) | `repos-manager` (built from source) | `yay -S repos-manager` |

The AUR packages ship the bash, zsh and fish completions. Update package-managed installs with the package manager itself: `repos-manager update` detects them and prints the right command.

### Linux and macOS

```bash
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/installers/install.sh | sh
```

The script picks the archive for your OS and CPU, checks it against `SHA256SUMS` and installs the binary into `~/.local/bin`. Options go after `sh -s --`:

| Option | Variable | Default | Description |
|--------|----------|---------|-------------|
| `--version vX.Y.Z` | `REPOS_MANAGER_VERSION` | latest | Release to install |
| `--prefix DIR` | `PREFIX` | `~/.local` | Installs into `DIR/bin` |
| `--source github\|forge` | | `github` | Where to download from |
| `--uninstall` | | | Remove the installed binary |

```bash
# Pin a version, system-wide
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/installers/install.sh | sudo sh -s -- --version v1.0.0 --prefix /usr/local

# Use the forge as source (Linux only, macOS builds are only published on GitHub)
curl -fsSL https://forge.infrasouveraine.fr/dxsk/repos-manager/raw/branch/main/installers/install.sh | sh -s -- --source forge
```

### Windows

With PowerShell:

```powershell
irm https://raw.githubusercontent.com/Dxsk/repos-manager/main/installers/install.ps1 | iex
```

It installs `repos-manager.exe` into `%LOCALAPPDATA%\Programs\repos-manager` and adds that folder to your user `PATH`. Open a new terminal afterwards. The script accepts `-Version`, `-Source github|forge`, `-InstallDir` and `-Uninstall`; to pass them without saving it first:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/Dxsk/repos-manager/main/installers/install.ps1))) -Version v1.0.0
```

Setting `$env:REPOS_MANAGER_VERSION = "v1.0.0"` before the one-liner pins a version too.

Prefer an installer? Download `repos-manager-x86_64-setup.exe` (or `repos-manager-aarch64-setup.exe` on ARM devices) from the [latest release](https://github.com/Dxsk/repos-manager/releases/latest). It installs for the current user without admin rights, adds the folder to `PATH` and registers an uninstaller in *Apps and features*. Silent install: `repos-manager-x86_64-setup.exe /S`.

The portable `repos-manager-x86_64-pc-windows-msvc.zip` and `repos-manager-aarch64-pc-windows-msvc.zip` archives work too: put `repos-manager.exe` somewhere on your `PATH`.

### From source

Requires a recent stable [Rust toolchain](https://rustup.rs) and `make`:

```bash
git clone https://github.com/Dxsk/repos-manager.git
cd repos-manager
make install        # cargo build --release, then copy to ~/.local/bin
make completions    # bash, zsh and fish completions in your user directories
```

Change the prefix with `make install PREFIX=/usr/local` (packagers can also set `DESTDIR`). Remove everything with `make uninstall`.

Or install with cargo, from crates.io or straight from the repository:

```bash
cargo install repos-manager
cargo install --git https://github.com/Dxsk/repos-manager.git
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
