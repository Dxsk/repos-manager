# repos-manager

[![CI](https://github.com/Dxsk/repos-manager/actions/workflows/ci.yml/badge.svg)](https://github.com/Dxsk/repos-manager/actions/workflows/ci.yml)
[![GitHub Release](https://img.shields.io/github/v/release/Dxsk/repos-manager)](https://github.com/Dxsk/repos-manager/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Buy Me A Coffee](https://img.shields.io/badge/Buy%20Me%20A%20Coffee-donate-ffdd00.svg?logo=buymeacoffee&logoColor=black)](https://buymeacoffee.com/dxsk)

A single CLI tool to clone and sync all your Git repositories, no matter the provider.

`repos-manager` is one native binary for Linux, macOS and Windows. It drives the official CLI of each provider and mirrors every repository you can access into `<base_dir>/<host>/<owner>/<repo>`.

> **Where the project lives:** development happens on the Forgejo forge at [forge.infrasouveraine.fr/dxsk/repos-manager](https://forge.infrasouveraine.fr/dxsk/repos-manager) (issues, pull requests, releases). [GitHub](https://github.com/Dxsk/repos-manager) is a read-only mirror of `main` and tags, and also hosts the releases.

Full documentation: <https://repos-manager.dxscloud.fr>

## Supported providers

| Provider | CLI | Status |
|----------|-----|--------|
| GitHub | `gh` | Done |
| GitLab | `glab` | Done |
| Forgejo / Gitea | `tea` | Done |
| Bitbucket | `bitbucket` or API | Done |
| Radicle | `rad` | Done |

## Features

- Single native binary for Linux (x86_64, aarch64), macOS (x86_64, aarch64) and Windows (x86_64)
- Clone all accessible repos (personal, collaborations, orgs, groups, subgroups)
- Mirror the remote namespace hierarchy locally: `host/owner/repo`
- Update existing repos with fetch + fast-forward pull of `origin HEAD`
- Skip repos with uncommitted local changes
- Remove local repos that no longer exist on the remote (`--prune`)
- Preview changes before applying them (`--dry-run`)
- SSH and HTTPS support
- Filter by owner or specific repo (`--filter`)
- Exclude repos via `.repos-ignore`, restrict them via `.repos-filter`
- Parallel sync (default: 4 jobs, configurable with `--parallel`)
- Several hosts per provider (SaaS and self-hosted side by side)
- Per-host lock to prevent concurrent syncs of the same host, released automatically if the process dies
- Verbose (`--verbose`) and quiet (`--quiet`) output modes
- Status overview: dirty, ahead, behind, diverged repos
- Fast `status` scan with live progress indicator, automatic skip of cloud drives and other network mounts
- Background update check with a one-line banner, and a self-update that verifies checksums
- Universal login: authenticate all detected providers at once
- Config file (`~/.config/repos-manager/config.json`) for defaults
- Shell completions for bash, zsh, fish, PowerShell and elvish
- Per-provider help (`repos-manager github --help`)
- `NO_COLOR` support

## Directory structure

After syncing, your workspace looks like this:

```
~/Documents/
  .repos-filter
  .repos-ignore
  github.com/
    dxsk/
      my-project/
    my-org/
      other-project/
  gitlab.com/
    my-user/
      project/
    my-group/
      sub-group/
        project/
```

On Windows, a host with a port (`git.example.org:3000`) is stored as `git.example.org_3000`, since `:` is not allowed in directory names.

## Installation

Release archives (`repos-manager-{linux,macos}-{x86_64,aarch64}.tar.gz`, `repos-manager-windows-x86_64.zip`, the Windows installer and `SHA256SUMS`) are published on [GitHub Releases](https://github.com/Dxsk/repos-manager/releases) and on the [forge](https://forge.infrasouveraine.fr/dxsk/repos-manager/releases). See [`install/README.md`](install/README.md) for every option.

### Linux and macOS

```bash
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.sh | sh
```

The script picks the archive for your OS and CPU, verifies it against `SHA256SUMS` and installs the binary into `~/.local/bin`. Options go after `sh -s --`:

```bash
# Pin a version
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.sh | sh -s -- --version v1.0.0

# Install system-wide
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.sh | sudo sh -s -- --prefix /usr/local

# Download from the forge instead of GitHub (Linux only)
curl -fsSL https://forge.infrasouveraine.fr/dxsk/repos-manager/raw/branch/main/install/install.sh | sh -s -- --source forge

# Uninstall
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.sh | sh -s -- --uninstall
```

### Windows

With PowerShell:

```powershell
irm https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.ps1 | iex
```

This installs `repos-manager.exe` into `%LOCALAPPDATA%\Programs\repos-manager` and adds it to your user `PATH` (open a new terminal afterwards).

Or download `repos-manager-windows-x86_64-setup.exe` from the [latest release](https://github.com/Dxsk/repos-manager/releases/latest) and run it. It installs for the current user without admin rights, updates `PATH` and registers an uninstaller.

### From source

Requires a recent stable [Rust toolchain](https://rustup.rs) and `make`:

```bash
git clone https://forge.infrasouveraine.fr/dxsk/repos-manager.git
cd repos-manager
make -C install install        # builds and installs into ~/.local/bin
make -C install completions    # bash, zsh and fish completions
```

Change the prefix with `make -C install install PREFIX=/usr/local`.

Or let cargo do it:

```bash
cargo install --git https://forge.infrasouveraine.fr/dxsk/repos-manager.git
```

## Requirements

- `git`
- The CLI of each provider you sync:
  - `gh` for GitHub
  - `glab` for GitLab
  - `tea` for Forgejo / Gitea (used for login only, see [providers](site/src/docs/providers.md))
  - `bitbucket` for Bitbucket (optional, an API fallback is built in)
  - `rad` for Radicle

Nothing else: no `bash`, `jq`, `yq` or `curl` needed at runtime.

## Usage

### Configuration

Generate a default config file:

```bash
repos-manager init
# Creates ~/.config/repos-manager/config.json
```

```json
{
  "base_dir": "~/Documents",
  "parallel": 4,
  "protocol": "ssh",
  "check_updates": true,
  "scan_network_mounts": false,
  "hosts": {
    "github":    ["github.com"],
    "gitlab":    ["gitlab.com"],
    "forgejo":   ["codeberg.org"],
    "bitbucket": ["bitbucket.org"]
  }
}
```

The path is the same on every OS, including Windows (`%USERPROFILE%\.config\repos-manager\config.json`). Override it with `REPOS_MANAGER_CONFIG`.

Two behaviours are controlled from the config file:

- `check_updates` (default `true`): see [Updating](#updating).
- `scan_network_mounts` (default `false`): on Linux, `repos-manager status` reads `/proc/self/mountinfo` and prunes every mount point under `base_dir` whose filesystem type is `fuse`, `fuse.*`, `nfs`, `cifs`, `smb*`, `smbfs`, `afs`, `ceph` or `davfs`. This keeps the scan fast on setups that host cloud drives (kDrive, Dropbox, sshfs) under `~/Documents`. Set it to `true` if you really do host repos on a reliable network share; the scan then prints a warning that it can hang on flaky links.

Each `hosts.<provider>` key takes a **list** of hostnames, so you can sync several instances of the same provider (typically a SaaS one plus your self-hosted one):

```json
{
  "hosts": {
    "gitlab":  ["gitlab.com", "gitlab.example.org"],
    "forgejo": ["codeberg.org", "forge.example.org"]
  }
}
```

Each instance is cloned under its own directory (`<base_dir>/<host>/...`). For self-hosted instances you need a matching CLI login: `gh auth login --hostname <host>` for GitHub Enterprise, `glab auth login --hostname <host>` for GitLab, `tea login add` for Forgejo. The legacy single-string form (`"gitlab": "gitlab.com"`) is still accepted.

### Authentication

```bash
# Login all detected providers at once
repos-manager login

# Or login a specific provider
repos-manager login github
repos-manager github login
repos-manager gitlab login
repos-manager forgejo login
repos-manager bitbucket login
repos-manager radicle login
```

### Syncing repos

```bash
# Sync all repos from GitHub
repos-manager github sync

# Sync all repos from GitLab
repos-manager gitlab sync

# Sync all configured providers at once
repos-manager sync --all

# Parallel sync with 8 jobs
repos-manager sync --all --parallel 8
```

`sync --all` only runs providers whose CLI (or Bitbucket credentials) is available, and one failing host never stops the others.

### Status

Check which repos have uncommitted changes, are ahead/behind, or diverged:

```bash
repos-manager status
```

```
  github.com/Dxsk/dotenv dirty
  github.com/Dxsk/mtd ahead (+2)
  gitlab.com/work/api behind (-3)

Total: 42 repos - 39 clean, 1 dirty, 1 ahead, 1 behind, 0 diverged
```

### Filtering

```bash
# Sync only repos from a specific owner
repos-manager github sync --filter 'Dxsk/*'

# Sync a single repo
repos-manager github sync --filter Dxsk/repos-manager
```

### Other options

```bash
# Use HTTPS instead of SSH
repos-manager github sync --https

# Remove local repos that no longer exist on the remote
repos-manager sync --all --prune

# Preview what would happen without making changes
repos-manager sync --all --dry-run

# GitLab self-hosted
repos-manager gitlab sync --host gitlab.self-hosted.com

# Forgejo / Gitea (`gitea` is an alias of `forgejo`)
repos-manager forgejo sync --host forgejo.self-hosted.com

# Custom base directory
repos-manager sync --all --base-dir /path/to/repos
```

`--host` targets a single provider, so it is rejected with `sync --all`. `--prune` is skipped when `--filter` is set and never deletes anything outside the base directory.

## Flags

Sync flags (`<provider> sync` and `sync --all`):

| Flag | Description |
|------|-------------|
| `--filter <pattern>` | Filter repos by pattern (e.g. `Dxsk/*` or `Dxsk/project`) |
| `--base-dir <path>` | Base directory for repos (default: `~/Documents`) |
| `--https` | Use HTTPS clone URLs instead of SSH |
| `--prune` | Remove local repos not found on the remote |
| `--dry-run` | Show what would be done without making any changes |
| `--host <host>` | Custom host for self-hosted instances (provider sync only) |
| `--parallel <n>` | Number of parallel sync jobs (default: 4) |
| `--verbose`, `-v` | Show debug output |
| `--quiet`, `-q` | Suppress info/success messages (errors still shown) |

`status` accepts `--base-dir`, `--verbose` and `--quiet`. `update` accepts `--yes` / `-y`.

## Shell completions

Completions are generated by the binary itself:

```bash
# bash
repos-manager completions bash > ~/.local/share/bash-completion/completions/repos-manager

# zsh (make sure the directory is in your fpath)
repos-manager completions zsh > ~/.local/share/zsh/site-functions/_repos-manager

# fish
repos-manager completions fish > ~/.config/fish/completions/repos-manager.fish
```

```powershell
# PowerShell (add this line to your $PROFILE to make it persistent)
repos-manager completions powershell | Out-String | Invoke-Expression
```

`elvish` is supported too. From a source checkout, `make -C install completions` installs the bash, zsh and fish files for you.

## Updating

Every command that touches repositories prints a one-line banner when a newer release is out:

```
⬆ repos-manager 1.1.0 available (current 1.0.0), run: repos-manager update
```

The banner reads a cached version. The cache is refreshed at most once a day by a detached background process querying the GitHub Releases API, so the check never slows down the command in progress. Disable it with `"check_updates": false` in the config file, or `REPOS_MANAGER_NO_UPDATE_CHECK=1` for a single run.

To update:

```bash
repos-manager update        # asks for confirmation
repos-manager update --yes  # no prompt
```

`update` downloads the release archive for your platform, verifies it against the release `SHA256SUMS` and replaces the binary in place. Re-running the install script works too.

## Filter and ignore files

Both files live at the root of the base directory.

### .repos-filter

Sync **only** repos matching at least one pattern. If the file is missing, all repos are synced; if it exists but holds no pattern, nothing is synced.

```
# Only sync repos from Dxsk
Dxsk/*

# Plus a specific repo from another org
other-org/some-project
```

### .repos-ignore

Exclude repos from syncing. Applied **after** `.repos-filter`.

```
# Ignore a specific repo
Dxsk/old-project

# Ignore all repos from an owner
test-org/*
```

### Pattern syntax

- Glob wildcards: `*`, `?`
- `owner/*` also matches nested paths (e.g. `group/subgroup/project`)
- `#` starts a comment
- Empty lines are ignored

## Environment variables

| Variable | Description | Default |
|----------|-------------|---------|
| `REPOS_MANAGER_CONFIG` | Path to the config file | `~/.config/repos-manager/config.json` |
| `REPOS_MANAGER_BASE_DIR` | Base directory for all repos | `~/Documents` |
| `REPOS_MANAGER_PARALLEL` | Default parallel jobs | `4` |
| `REPOS_MANAGER_PROTOCOL` | Default protocol (`ssh` or `https`) | `ssh` |
| `REPOS_MANAGER_NO_UPDATE_CHECK` | Set to `1` to skip the background update check | Unset |
| `REPOS_MANAGER_UPDATE_TTL` | Seconds between update checks | `86400` |
| `REPOS_MANAGER_UPDATE_CACHE` | Path to the cached latest-version file | `<cache dir>/repos-manager/latest-version` |
| `REPOS_MANAGER_UPDATE_URL` | Release API endpoint used by the update check and `update` | GitHub Releases API |
| `TEA_CONFIG` | Path to tea's config file, read by the Forgejo provider | tea's default location |
| `NO_COLOR` | Disable colored output when set | Unset |

Precedence: flag > environment variable > config file > default.

## Glossary

### Providers

| Provider | Description | CLI | Documentation |
|----------|-------------|-----|---------------|
| [GitHub](https://github.com) | The most popular Git hosting platform | [`gh`](https://cli.github.com/) | [GitHub CLI Manual](https://cli.github.com/manual/) |
| [GitLab](https://gitlab.com) | DevOps platform with built-in CI/CD | [`glab`](https://gitlab.com/gitlab-org/cli) | [GLab Documentation](https://gitlab.com/gitlab-org/cli/-/blob/main/README.md) |
| [Forgejo](https://forgejo.org) | Community-driven self-hosted Git forge (Gitea fork) | [`tea`](https://gitea.com/gitea/tea) | [Forgejo Documentation](https://forgejo.org/docs/latest/) |
| [Gitea](https://gitea.com) | Lightweight self-hosted Git service | [`tea`](https://gitea.com/gitea/tea) | [Gitea Documentation](https://docs.gitea.com/) |
| [Bitbucket](https://bitbucket.org) | Atlassian's Git platform (Cloud & Server) | [`bitbucket`](https://crates.io/crates/bitbucket-cli) | [Bitbucket API](https://developer.atlassian.com/cloud/bitbucket/rest/intro/) |
| [Radicle](https://radicle.xyz) | Sovereign peer-to-peer code forge built on Git | [`rad`](https://radicle.xyz/guides/user) | [Radicle User Guide](https://radicle.xyz/guides/user) |

### Tools

| Tool | Description | Documentation |
|------|-------------|---------------|
| [Git](https://git-scm.com) | Distributed version control system | [Git Reference](https://git-scm.com/docs) |
| [Rust](https://www.rust-lang.org) | Language repos-manager is written in | [The Rust Book](https://doc.rust-lang.org/book/) |

## Contributing

Issues and pull requests go to the [forge](https://forge.infrasouveraine.fr/dxsk/repos-manager/issues). See [CONTRIBUTING.md](CONTRIBUTING.md).

## Support

If this tool saves you time, you can support my work by [buying me a coffee](https://buymeacoffee.com/dxsk). Thank you!

## License

[MIT](LICENSE)
