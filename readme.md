<div align="center">

# repos-manager

**Clone and sync every Git repository you can access, on every provider, with one command.**

[![Forgejo CI](https://forge.infrasouveraine.fr/dxsk/repos-manager/badges/workflows/ci.yml/badge.svg?label=Forgejo%20CI)](https://forge.infrasouveraine.fr/dxsk/repos-manager/actions?workflow=ci.yml)
[![GitHub CI](https://img.shields.io/github/actions/workflow/status/Dxsk/repos-manager/ci.yml?branch=main&label=GitHub%20CI&logo=github)](https://github.com/Dxsk/repos-manager/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Dxsk/repos-manager?logo=git&logoColor=white)](https://github.com/Dxsk/repos-manager/releases/latest)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-informational)](#installation)
[![License](https://img.shields.io/github/license/Dxsk/repos-manager)](LICENSE)

[GitHub](https://github.com/Dxsk/repos-manager) · [Documentation](https://repos-manager.dxscloud.fr) · [Upstream (Forgejo)](https://forge.infrasouveraine.fr/dxsk/repos-manager)

</div>

---

If your code is spread over GitHub, a GitLab group, a self-hosted Forgejo and a couple of Bitbucket workspaces, keeping a local copy of everything up to date is tedious. `repos-manager` asks each provider for every repository you can access (personal, collaborations, organizations, groups and subgroups) and mirrors them into `<base_dir>/<host>/<owner>/<repo>`, cloning what is missing and fast-forwarding the rest.

It is a single native binary for Linux, macOS and Windows. At runtime it only needs `git` and the official CLI of each provider you sync.

```console
$ repos-manager sync --all
$ repos-manager status
  github.com/Dxsk/dotenv dirty
  github.com/Dxsk/mtd ahead (+2)
  gitlab.com/work/api behind (-3)

Total: 42 repos - 39 clean, 1 dirty, 1 ahead, 1 behind, 0 diverged
```

## Supported providers

| Provider | CLI | Notes |
|---|---|---|
| GitHub | [`gh`](https://cli.github.com/) | github.com and GitHub Enterprise |
| GitLab | [`glab`](https://gitlab.com/gitlab-org/cli) | Groups and nested subgroups |
| Forgejo / Gitea | [`tea`](https://gitea.com/gitea/tea) | `tea` is used for login only, listing goes through the REST API |
| Bitbucket | [`bitbucket`](https://crates.io/crates/bitbucket-cli) | Optional, an API fallback is built in |
| Radicle | [`rad`](https://radicle.xyz/guides/user) | |

## Installation

Prebuilt binaries for every release are on the [GitHub releases page](https://github.com/Dxsk/repos-manager/releases), which has every platform. The [Forgejo releases](https://forge.infrasouveraine.fr/dxsk/repos-manager/releases) carry the Linux and Windows x86_64 builds.

<details open>
<summary><b>Linux and macOS</b></summary>

<br>

```sh
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/installers/install.sh | sh
```

The script downloads the archive that matches your OS and CPU, checks it against the release's `SHA256SUMS`, and puts the binary in `~/.local/bin`. Options go after `sh -s --`, and most of them also have an environment variable:

| Option | Variable | Default | |
|---|---|---|---|
| `--version vX.Y.Z` | `REPOS_MANAGER_VERSION` | latest | Release to install |
| `--prefix DIR` | `PREFIX` | `~/.local` | Installs into `DIR/bin` |
| `--source github\|forge` | | `github` | Where to download from |
| `--uninstall` | | | Removes the installed binary |

```sh
# Pin a version, system-wide
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/installers/install.sh | sudo sh -s -- --version v1.0.0 --prefix /usr/local

# Download from the forge instead of GitHub (Linux only, macOS builds are on GitHub)
curl -fsSL https://forge.infrasouveraine.fr/dxsk/repos-manager/raw/branch/main/installers/install.sh | sh -s -- --source forge

# Uninstall
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/installers/install.sh | sh -s -- --uninstall
```

</details>

<details open>
<summary><b>Windows</b></summary>

<br>

With PowerShell:

```powershell
irm https://raw.githubusercontent.com/Dxsk/repos-manager/main/installers/install.ps1 | iex
```

It installs `repos-manager.exe` into `%LOCALAPPDATA%\Programs\repos-manager` and adds that folder to your user `PATH`. Open a new terminal afterwards.

The script accepts `-Version`, `-Source github|forge`, `-InstallDir` and `-Uninstall`. To pass them without saving the file first:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/Dxsk/repos-manager/main/installers/install.ps1))) -Version v1.0.0
```

Setting `$env:REPOS_MANAGER_VERSION = "v1.0.0"` before the one-liner pins a version too.

Prefer an installer? Download and run `repos-manager-x86_64-setup.exe` (or `repos-manager-aarch64-setup.exe` on ARM devices). It installs per user, so it does not ask for admin rights, adds the program to your `PATH` and shows up in *Apps & features* if you want to remove it later. For a silent install, pass `/S`:

```powershell
.\repos-manager-x86_64-setup.exe /S
```

If you would rather not install anything, grab the portable `.zip` and put `repos-manager.exe` somewhere on your `PATH`.

</details>

<details>
<summary><b>From source</b></summary>

<br>

You need a stable [Rust toolchain](https://rustup.rs) and `make`.

```sh
git clone https://github.com/Dxsk/repos-manager.git
cd repos-manager
make install                    # builds and installs to ~/.local/bin/repos-manager
make install PREFIX=/usr/local  # or anywhere else
make completions                # bash, zsh and fish completions in your user directories
make uninstall
```

Packagers can stage the install with `DESTDIR`. Cargo works too:

```sh
cargo install --git https://github.com/Dxsk/repos-manager.git
```

</details>

<details>
<summary><b>Release assets</b></summary>

<br>

Assets are named after the Rust target they were built for:

| Asset | Platform | GitHub | Forgejo |
|---|---|:---:|:---:|
| `repos-manager-x86_64-unknown-linux-musl.tar.gz` | Linux x86_64 (static) | ✓ | ✓ |
| `repos-manager-aarch64-unknown-linux-musl.tar.gz` | Linux aarch64 (static) | ✓ | ✓ |
| `repos-manager-x86_64-apple-darwin.tar.gz` | macOS Intel | ✓ | |
| `repos-manager-aarch64-apple-darwin.tar.gz` | macOS Apple Silicon | ✓ | |
| `repos-manager-x86_64-pc-windows-msvc.zip` | Windows x86_64 | ✓ | |
| `repos-manager-aarch64-pc-windows-msvc.zip` | Windows ARM64 | ✓ | |
| `repos-manager-x86_64-pc-windows-gnu.zip` | Windows x86_64 (MinGW build) | | ✓ |
| `repos-manager-x86_64-setup.exe` | Windows x86_64 installer | ✓ | ✓ |
| `repos-manager-aarch64-setup.exe` | Windows ARM64 installer | ✓ | |

</details>

<details>
<summary><b>Verifying a download</b></summary>

<br>

Every release ships two checksum files:

- `SHA256SUMS` lists the archives and installers you download.
- `SHA256SUMS-binaries` lists the `repos-manager` binary inside each archive. Use it to check a binary you already extracted or installed.

On Linux and macOS:

```sh
sha256sum --ignore-missing -c SHA256SUMS   # macOS: shasum -a 256 --ignore-missing -c SHA256SUMS
```

On Windows:

```powershell
(Get-FileHash .\repos-manager-x86_64-setup.exe).Hash.ToLower()
(Get-FileHash "$env:LOCALAPPDATA\Programs\repos-manager\repos-manager.exe").Hash.ToLower()
```

Then compare the result with the matching line in the checksum file. The install scripts and `repos-manager update` do this for you.

</details>

### Requirements

- `git`
- The CLI of each provider you sync: `gh`, `glab`, `tea`, `bitbucket` (optional) or `rad`

Nothing else: no `bash`, `jq`, `yq` or `curl` needed at runtime.

## Getting started

```sh
repos-manager init         # optional, writes ~/.config/repos-manager/config.json
repos-manager login        # authenticates every provider whose CLI is installed
repos-manager sync --all   # clones and updates everything
repos-manager status       # shows dirty, ahead, behind and diverged repos
```

After a sync, your workspace mirrors the remote namespaces:

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
    my-group/
      sub-group/
        project/
```

On Windows, a host with a port (`git.example.org:3000`) is stored as `git.example.org_3000`, since `:` is not allowed in directory names.

## Usage

### Commands

| Command | What it does |
|---|---|
| `repos-manager login [<provider>]` | Logs in to one provider, or to every detected one |
| `repos-manager <provider> login` | Same, per provider |
| `repos-manager <provider> sync` | Syncs one provider (`github`, `gitlab`, `forgejo`, `gitea`, `bitbucket`, `radicle`) |
| `repos-manager sync --all` | Syncs every configured provider |
| `repos-manager status` | Lists repos that are dirty, ahead, behind or diverged |
| `repos-manager init` | Creates the config file |
| `repos-manager update` | Updates the binary in place |
| `repos-manager completions <shell>` | Prints a completion script |
| `repos-manager version` | Prints the version |

`sync --all` only runs providers whose CLI (or Bitbucket credentials) is available, and one failing host never stops the others. Every provider has its own help: `repos-manager github --help`.

### Sync flags

| Flag | Description |
|---|---|
| `--filter <pattern>` | Only sync matching repos (`Dxsk/*` or `Dxsk/project`) |
| `--base-dir <path>` | Base directory for repos (default: `~/Documents`) |
| `--https` | Use HTTPS clone URLs instead of SSH |
| `--prune` | Remove local repos that no longer exist on the remote |
| `--dry-run` | Show what would be done without changing anything |
| `--host <host>` | Target a self-hosted instance (provider sync only) |
| `--parallel <n>` | Number of parallel jobs (default: 4) |
| `-v`, `--verbose` | Show debug output |
| `-q`, `--quiet` | Hide info and success messages (errors still show) |

`status` accepts `--base-dir`, `--verbose` and `--quiet`. `update` accepts `-y` / `--yes`.

### Examples

```sh
# Only one owner, or a single repo
repos-manager github sync --filter 'Dxsk/*'
repos-manager github sync --filter Dxsk/repos-manager

# Preview, then clean up repos deleted on the remote
repos-manager sync --all --prune --dry-run
repos-manager sync --all --prune

# Self-hosted instances (gitea is an alias of forgejo)
repos-manager gitlab sync --host gitlab.example.org
repos-manager forgejo sync --host forge.example.org

# HTTPS, more jobs, another base directory
repos-manager sync --all --https --parallel 8 --base-dir /path/to/repos
```

Existing clean repos get `git fetch --all` and a fast-forward pull of `origin HEAD`. Repos with uncommitted changes are skipped. `--host` is rejected with `sync --all`, and `--prune` is skipped when `--filter` is set and never deletes anything outside the base directory.

Each host has its own lock, so two syncs of the same host cannot overlap while different hosts sync side by side. The lock is released by the OS if the process dies.

## Configuration

<details open>
<summary><b>Config file</b></summary>

<br>

`repos-manager init` creates `~/.config/repos-manager/config.json`. The path is the same on every OS, including Windows (`%USERPROFILE%\.config\repos-manager\config.json`), and `REPOS_MANAGER_CONFIG` overrides it.

```json
{
  "base_dir": "~/Documents",
  "parallel": 4,
  "protocol": "ssh",
  "check_updates": true,
  "scan_network_mounts": false,
  "hosts": {
    "github":    ["github.com"],
    "gitlab":    ["gitlab.com", "gitlab.example.org"],
    "forgejo":   ["codeberg.org", "forge.example.org"],
    "bitbucket": ["bitbucket.org"]
  }
}
```

| Field | Default | Description |
|---|---|---|
| `base_dir` | `~/Documents` | Where repos are cloned |
| `parallel` | `4` | Parallel sync jobs |
| `protocol` | `ssh` | `ssh` or `https` |
| `check_updates` | `true` | Background update check, see [Updating](#updating) |
| `scan_network_mounts` | `false` | Let `status` walk network and FUSE mounts (Linux) |
| `hosts.<provider>` | provider's SaaS host | One hostname or a list. Each one is synced into its own `<base_dir>/<host>/` |

Self-hosted instances need a matching CLI login: `gh auth login --hostname <host>`, `glab auth login --hostname <host>` or `tea login add`.

Precedence: flag > environment variable > config file > default.

</details>

<details>
<summary><b>Filter and ignore files</b></summary>

<br>

Both files live at the root of the base directory and use the same patterns: `*` and `?` wildcards, `owner/*` also matches nested paths (`group/subgroup/project`), `#` starts a comment and empty lines are ignored.

`.repos-filter` syncs **only** repos matching at least one pattern. If the file is missing, everything is synced; if it exists but holds no pattern, nothing is.

```
# Only my repos, plus one from another org
Dxsk/*
other-org/some-project
```

`.repos-ignore` excludes repos, and is applied **after** `.repos-filter`.

```
Dxsk/old-project
test-org/*
```

</details>

<details>
<summary><b>Environment variables</b></summary>

<br>

| Variable | Description | Default |
|---|---|---|
| `REPOS_MANAGER_CONFIG` | Path to the config file | `~/.config/repos-manager/config.json` |
| `REPOS_MANAGER_BASE_DIR` | Base directory for all repos | `~/Documents` |
| `REPOS_MANAGER_PARALLEL` | Default parallel jobs | `4` |
| `REPOS_MANAGER_PROTOCOL` | Default protocol (`ssh` or `https`) | `ssh` |
| `REPOS_MANAGER_NO_UPDATE_CHECK` | Set to `1` to skip the background update check | unset |
| `REPOS_MANAGER_UPDATE_TTL` | Seconds between update checks | `86400` |
| `REPOS_MANAGER_UPDATE_CACHE` | Path to the cached latest-version file | `<cache dir>/repos-manager/latest-version` |
| `REPOS_MANAGER_UPDATE_URL` | Release API used by the update check and `update` | GitHub Releases API |
| `TEA_CONFIG` | Path to tea's config file, read by the Forgejo provider | tea's default location |
| `NO_COLOR` | Disable colored output | unset |

</details>

<details>
<summary><b>Fast status on large workspaces</b></summary>

<br>

`status` skips vendored directories (`node_modules`, `.venv`, `target`, `vendor`, `dist` and friends) and shows a live progress line while it scans.

On Linux it also reads `/proc/self/mountinfo` and prunes every mount under `base_dir` whose filesystem is `fuse`, `fuse.*`, `nfs`, `cifs`, `smb*`, `smbfs`, `afs`, `ceph` or `davfs`. Cloud drives (kDrive, Dropbox, sshfs) mounted under `~/Documents` would otherwise make the scan crawl. Set `"scan_network_mounts": true` if you really host repos on a reliable network share.

</details>

## Shell completions

The binary generates them for bash, zsh, fish, PowerShell and elvish:

```sh
repos-manager completions bash > ~/.local/share/bash-completion/completions/repos-manager
repos-manager completions zsh  > ~/.local/share/zsh/site-functions/_repos-manager
repos-manager completions fish > ~/.config/fish/completions/repos-manager.fish
```

```powershell
# Add this line to your $PROFILE to keep it
repos-manager completions powershell | Out-String | Invoke-Expression
```

From a source checkout, `make completions` installs the bash, zsh and fish files for you.

## Updating

Commands that touch repositories print a one-line banner when a newer release is out:

```
⬆ repos-manager 1.1.0 available (current 1.0.0), run: repos-manager update
```

The banner reads a cached version, refreshed at most once a day by a detached background process that queries the GitHub Releases API, so the check never slows down the command in progress. Turn it off with `"check_updates": false`, or with `REPOS_MANAGER_NO_UPDATE_CHECK=1` for a single run.

```sh
repos-manager update      # asks for confirmation
repos-manager update -y   # no prompt
```

`update` downloads the GitHub release asset for your platform (the `msvc` zip on Windows), verifies it against the release's `SHA256SUMS` and replaces the binary in place. Re-running the install script works too.

### Upgrading from 0.x

1.0 is a breaking release: the Bash tool became a native binary, and the 0.x `repos-manager update` cannot install it. Your config file, `.repos-filter`, `.repos-ignore` and synced repositories are kept as they are.

```sh
# 1. Remove the 0.x Bash install (make install put it here)
rm -f ~/.local/bin/repos-manager
rm -rf ~/.local/lib/repos-manager

# 2. Install 1.x
curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/installers/install.sh | sh

# 3. Shell integration: drop the old `source .../sourceme*` lines from your
#    shell rc file, delete the generated files, and use completions instead
find ~/Documents -maxdepth 2 -name 'sourceme*' -delete   # adjust to your base_dir
# then set up completions with: repos-manager completions <shell> (see Shell completions)
```

Nix users: the flake is gone, use one of the install methods above. If you set `REPOS_MANAGER_VERSION_URL`, it is now `REPOS_MANAGER_UPDATE_URL` and points to a GitHub Releases API URL.

## Development

<details>
<summary><b>Building and testing</b></summary>

<br>

```sh
make build     # optimized binary in target/release/
make test      # unit and integration tests
make coverage  # tests plus line coverage (needs cargo-llvm-cov)
make lint      # rustfmt and clippy
```

Tests shell out to the real `git` with bare remotes in temporary directories, so they need git 2.28 or newer. Run the CLI without installing it with `cargo run -- github sync --dry-run --base-dir /tmp/rm`.

To build the Windows installer yourself, install [NSIS](https://nsis.sourceforge.io) and run:

```powershell
cargo build --release
makensis /DVERSION=1.0.0 /DARCH=x86_64 /DBINARY="$PWD\target\release\repos-manager.exe" installers\repos-manager.nsi
```

`repos-manager-x86_64-setup.exe` is written next to the script unless you pass `/DOUTFILE=path`.

The full contributing guide is in [CONTRIBUTING.md](CONTRIBUTING.md) and on the [documentation site](https://repos-manager.dxscloud.fr/docs/contributing/). Issues and pull requests go to [GitHub](https://github.com/Dxsk/repos-manager/issues). Report vulnerabilities privately through a [security advisory](https://github.com/Dxsk/repos-manager/security/advisories/new), see [SECURITY.md](SECURITY.md).

</details>

<details>
<summary><b>CI and releases</b></summary>

<br>

The project lives on [Forgejo](https://forge.infrasouveraine.fr/dxsk/repos-manager) and is push-mirrored to [GitHub](https://github.com/Dxsk/repos-manager), which only receives `main` and tags. Each side has its own pipeline:

| | Forgejo (`.forgejo/workflows/`) | GitHub mirror (`.github/workflows/`) |
|---|---|---|
| On every push | Format, lint, tests and coverage on Linux, ShellCheck | Format and lint, tests on Linux, macOS and Windows, ShellCheck, link check |
| On a `v*` tag | Linux x86_64 and aarch64 (musl), Windows x86_64 (MinGW) and its installer | Every asset: Linux, macOS (Intel and Apple Silicon), Windows x86_64 and ARM64 with both installers |

Both releases include `SHA256SUMS` and `SHA256SUMS-binaries`. The install script and `repos-manager update` download from GitHub by default, which is why the GitHub release carries every asset.

</details>

<details>
<summary><b>Cutting a release</b></summary>

<br>

1. Describe the changes under `## Unreleased` in [CHANGELOG.md](CHANGELOG.md).
2. Run the release target with the new version:

   ```sh
   make release V=1.1.0
   ```

   The working tree must be clean. It sets the version in `Cargo.toml`, `Cargo.lock` and the documentation site, turns the `Unreleased` heading into `## 1.1.0 (date)`, then commits `chore: release v1.1.0` and creates the annotated `v1.1.0` tag. Nothing is pushed yet, so you can still check the result.
3. Push to the forge:

   ```sh
   git push --follow-tags
   ```

The tag starts the Forgejo release, and the push mirror carries `main` and the tag to GitHub, which builds the full release. There is no automatic version bump: the version only changes through `make release`.

Since the mirror overwrites GitHub on every sync, never commit or tag directly on GitHub.

</details>

## License

[MIT](LICENSE)
