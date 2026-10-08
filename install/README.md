# Installing repos-manager

`repos-manager` is a single static binary. Pick the method that suits you.

At runtime you only need `git`, plus the CLI of each provider you sync
(`gh`, `glab`, `tea`, `bitbucket`, `rad`).

| Method | Platforms | Needs |
| --- | --- | --- |
| [Install script](#linux-and-macos-install-script) | Linux, macOS (x86_64, aarch64) | `curl` or `wget` |
| [PowerShell script](#windows-powershell) | Windows x86_64 | PowerShell 5.1+ |
| [Windows installer](#windows-installer) | Windows x86_64 | nothing |
| [From source](#from-source) | anything Rust supports | Rust toolchain, `make` |

Release archives and checksums are published on
[GitHub Releases](https://github.com/Dxsk/repos-manager/releases) and on the
[forge](https://forge.infrasouveraine.fr/dxsk/repos-manager/releases).

## Linux and macOS (install script)

```sh
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.sh | sh
```

The script picks the right archive for your OS and CPU, checks it against
`SHA256SUMS` and installs the binary into `~/.local/bin`.

Options are passed after `sh -s --`:

```sh
# Pin a version
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.sh | sh -s -- --version v1.0.0

# Install system-wide
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.sh | sudo sh -s -- --prefix /usr/local

# Download from the forge instead of GitHub
curl -fsSL https://forge.infrasouveraine.fr/dxsk/repos-manager/raw/branch/main/install/install.sh | sh -s -- --source forge

# Remove it
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.sh | sh -s -- --uninstall
```

| Option | Default | Description |
| --- | --- | --- |
| `--version vX.Y.Z` | latest (or `$REPOS_MANAGER_VERSION`) | Release to install |
| `--prefix DIR` | `$PREFIX` or `~/.local` | Installs into `DIR/bin` |
| `--source github\|forge` | `github` | Where to download from |
| `--uninstall` | | Removes `DIR/bin/repos-manager` |

> macOS builds are only published on GitHub, so `--source forge` works on Linux only.

## Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.ps1 | iex
```

It installs `repos-manager.exe` into `%LOCALAPPDATA%\Programs\repos-manager`
and adds that folder to your user `PATH` (open a new terminal afterwards).

To pin a version, set `$env:REPOS_MANAGER_VERSION = "v1.0.0"` first. When the
script is saved locally, it also accepts parameters:

```powershell
.\install.ps1 -Version v1.0.0 -Source forge
.\install.ps1 -Uninstall
```

## Windows installer

Download `repos-manager-windows-x86_64-setup.exe` from the latest release and
run it. It installs for the current user (no admin prompt), adds the install
folder to your `PATH` and registers an uninstaller in *Apps and features*.

Silent install: `repos-manager-windows-x86_64-setup.exe /S`

## From source

Requires a recent stable [Rust toolchain](https://rustup.rs).

```sh
git clone https://forge.infrasouveraine.fr/dxsk/repos-manager.git
cd repos-manager
make -C install install          # builds and installs into ~/.local/bin
make -C install completions      # bash, zsh and fish completions
```

| Target | Description |
| --- | --- |
| `install` | `cargo build --release --locked`, then copy to `$(PREFIX)/bin` |
| `uninstall` | Remove the binary and the completion files |
| `completions` | Write completions to `~/.local/share/bash-completion/completions/`, `~/.local/share/zsh/site-functions/` and `~/.config/fish/completions/` |

`PREFIX` defaults to `~/.local`; override it with `make -C install install PREFIX=/usr/local`.

Plain `cargo install --path .` works too.

## Shell completions

Any install method can generate completions on demand:

```sh
repos-manager completions bash > ~/.local/share/bash-completion/completions/repos-manager
repos-manager completions zsh  > ~/.local/share/zsh/site-functions/_repos-manager
repos-manager completions fish > ~/.config/fish/completions/repos-manager.fish
```

PowerShell:

```powershell
repos-manager completions powershell | Out-String | Invoke-Expression
```
