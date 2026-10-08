# Changelog

## Unreleased

Rewritten in Rust. repos-manager now ships as a single native binary for Linux (x86_64 and aarch64, static musl), macOS (x86_64 and aarch64) and Windows (x86_64 and ARM64). The commands, flags, config file (`~/.config/repos-manager/config.json`) and provider CLIs (`gh`, `glab`, `tea`, `bitbucket`, `rad`) are unchanged.

This is a breaking release: 0.x installs must be replaced (0.x `repos-manager update` cannot upgrade to it), the shell `sourceme` files are gone and the `install`/`update` flows changed. See "Upgrading from 0.x" in the readme and the Changed and Removed sections below.

### Added

- Native Windows support: config at `%USERPROFILE%\.config\repos-manager\config.json`, tea config read from `%LOCALAPPDATA%\tea\config.yml`, and hosts with a port (`host:3000`) stored as `host_3000`
- `repos-manager completions <bash|zsh|fish|powershell|elvish>` to print shell completion scripts
- `-y` / `--yes` on `repos-manager update` to skip the confirmation prompt
- `installers/install.sh` for Linux and macOS (`curl -fsSL .../installers/install.sh | sh`), installing the latest release to `~/.local/bin` after checking `SHA256SUMS`, with `--version` (or `REPOS_MANAGER_VERSION`), `--prefix` (or `PREFIX`), `--source github|forge` and `--uninstall`
- `installers/install.ps1` for Windows, installing to `%LOCALAPPDATA%\Programs\repos-manager` and adding it to the user `PATH`, with `-Version`, `-Source`, `-InstallDir` and `-Uninstall`
- Per-user NSIS installers, `repos-manager-x86_64-setup.exe` and `repos-manager-aarch64-setup.exe`
- Root `Makefile` with `build`, `test`, `coverage`, `lint`, `install`, `uninstall`, `completions` and `release` targets. Build from source with `make install` or `cargo install --git https://github.com/Dxsk/repos-manager.git`
- Release assets named by target triple: `repos-manager-<target>.tar.gz` for `x86_64`/`aarch64-unknown-linux-musl` and `x86_64`/`aarch64-apple-darwin`, `repos-manager-<target>.zip` for `x86_64`/`aarch64-pc-windows-msvc`, plus the installers, `SHA256SUMS` and `SHA256SUMS-binaries`
- Windows ARM64 builds (zip and installer) on the GitHub release
- Releases built on both forges: Forgejo publishes the Linux musl archives, an `x86_64-pc-windows-gnu` zip and the x86_64 installer; GitHub publishes every asset
- Test coverage is measured in the Forgejo CI (`make coverage` locally, needs `cargo-llvm-cov`)
- CI on both forges (Forgejo Actions on Linux, GitHub Actions on Linux, macOS and Windows)

### Changed

- Runtime dependencies are now `git` plus the provider CLIs only: `bash` 4+, `jq`, `yq` and `curl` are no longer needed
- `repos-manager update` downloads the GitHub release asset for the current platform (the `msvc` zip on Windows), verifies it against `SHA256SUMS` and replaces the binary in place, instead of pulling a git clone
- The update check queries the GitHub Releases API instead of fetching `VERSION` from `main`. `REPOS_MANAGER_VERSION_URL` is replaced by `REPOS_MANAGER_UPDATE_URL`
- `make install` builds the Rust binary (`PREFIX ?= ~/.local`, `DESTDIR` supported)
- `--host` with `sync --all` is rejected with a clear message instead of being ignored
- An invalid `config.json` is reported as an error instead of silently falling back to defaults
- The sync lock is per host (`<base_dir>/<host>/.repos-manager.lock`), so different hosts can sync concurrently (#5)
- The bats suite is replaced by `cargo test` (unit tests per module plus end-to-end CLI tests), linted with `cargo fmt` and `cargo clippy`
- Releases are cut by hand: changes go under `## Unreleased`, then `make release V=x.y.z` bumps the version, dates this file, commits and tags, and `git push --follow-tags` starts the releases. Versions are no longer bumped automatically from commit prefixes
- Development moves upstream to a Forgejo instance (https://forge.infrasouveraine.fr/dxsk/repos-manager), push-mirrored to GitHub with `main` and tags only. Issues, pull requests and security reports stay on GitHub (https://github.com/Dxsk/repos-manager), with private vulnerability reporting enabled
- The readme and every docs page describe the Rust binary: install methods per OS, shell completions, updating, Windows notes, tea config locations, the Makefile and the release process

### Removed

- The Bash sources
- The generated `sourceme`, `sourceme.zsh` and `sourceme.fish` files in host directories and the repo-root `sourceme.*` scripts, replaced by `repos-manager completions`. Leftover `sourceme*` files in existing host directories can be deleted
- The Nix flake
- `REPOS_MANAGER_LIB`
- The automatic version bump and changelog generation

### Fixed

- GitHub listing covers repos where you are a collaborator on another personal account (`/user/repos?affiliation=owner,collaborator,organization_member`) (#5)
- Pull `origin HEAD` explicitly, so clones without upstream tracking (manual clones, renamed default branch) are updated too (#5)
- The lock is an OS file lock released automatically when the process dies, so an interrupted sync never leaves a stale lock behind (#5)

## v0.6.0

- Add Buy Me a Coffee link

## v0.5.1

- Fix parse squash-merge bodies and strip bullet prefixes

## v0.5.0

### Providers

- Add multi-host support per provider: each `hosts.<provider>` key now accepts a list of hostnames so several GitLab or Forgejo instances can be synced side by side (#3)
- Forgejo / Gitea provider now lists organization repositories. Listing talks to the Forgejo REST API directly using credentials read from `~/.config/tea/config.yml`, paginating `/api/v1/user/repos`, `/api/v1/user/orgs` and `/api/v1/orgs/{org}/repos` (#4)
- Forgejo provider now requires `curl` and `yq` at runtime in addition to `jq`. Missing dependencies fail fast with a clear install hint (#4)
- Hosts with no matching `tea` login are now skipped with a warning instead of aborting the whole sync (#4)

### Status

- `repos-manager status` now prunes network and FUSE mount points under `base_dir` by default, parsing `/proc/self/mountinfo` to skip cloud drives (kDrive, Dropbox, sshfs), NFS, SMB/CIFS, davfs, Ceph and AFS shares. Set `scan_network_mounts: true` in the config file to opt back in (#4)
- `status` prunes heavy vendored directories (`node_modules`, `.venv`, `venv`, `__pycache__`, `target`, `vendor`, `dist`, `build`, `.next`, `.cache`) so working copies that host large dependency trees do not bloat the scan (#4)
- Add a live `[N] provider/owner/repo` progress indicator on stderr during the scan, auto-disabled when stderr is not a TTY or when `--quiet` is set (#4)
- Drop the internal `sort -z` on the `find` output so the scan streams into the progress indicator instead of buffering until traversal completes (#4)

### Update check

- Add a non-blocking background update check: every invocation except `update`, `version` and `help` spawns a detached `curl` that caches the latest published version for 24h under `${XDG_CACHE_HOME:-~/.cache}/repos-manager/latest-version`. The next run prints a one-line yellow banner when a newer release is available (#4)
- Add `check_updates` config option (default `true`) and `REPOS_MANAGER_NO_UPDATE_CHECK=1` environment variable to opt out of the background update check (#4)
- Add `REPOS_MANAGER_UPDATE_TTL` and `REPOS_MANAGER_UPDATE_CACHE` environment variables to tune the cache interval and location (#4)

### Fixes and portability

- Fix `log_info`, `log_success`, `log_skip` and `log_debug` aborting callers under `set -e`: a bare `return` after the enable predicate propagated the predicate's exit status, so disabled helpers returned 1 and killed any caller that ran outside a conditional context (#4)
- Fix `sync_provider` treating a provider list exit code of `2` as "skip this host" with a warning, instead of aborting the entire `sync --all` loop (#4)
- Fix several bash 3.2 parser quirks so the bats suite runs cleanly on the macOS CI runner: avoid `$(expr)` inside awk scripts, drop outer quotes around optional array expansions, and build the `find` command in an array outside the process substitution (#4)
- Install `curl`, `jq` and mikefarah `yq` explicitly on both Ubuntu and macOS CI runners so the Forgejo tests run with a consistent `yq` implementation (#4)

### Documentation

- Document the Forgejo API listing path, the update banner and its opt-outs, and the status network-mount skip in the getting-started, providers, configuration, usage and advanced docs (#4)
- Add a "Bash portability notes" section to the contributing page listing the bash 3.2 gotchas the project hits on macOS CI (#4)
- Refresh the contributing project tree to include `lib/update_check.sh`, `tests/update_check.bats` and `tests/lockfile.bats` (#4)
- Add a "Fast, safe status scan" feature card on the landing page and expand the Self-update card to mention the passive banner (#4)

## v0.4.2

- Fix CI version bump workflow now creates the GitHub release inline (the default `GITHUB_TOKEN` did not trigger `release.yml`)
- Fix CI version bump workflow now triggers the site deploy after a bump
- Auto-generate `CHANGELOG.md` entries during version bump via `.github/scripts/update-changelog.sh`
- Add `workflow_dispatch` to the Auto Version workflow so it can be triggered manually

## v0.4.1

- Fix `make install` produced an invalid `REPOS_MANAGER_LIB` path: `~` is not expanded inside `${VAR:-default}`, so the installed script tried to source the literal `~/.local/lib/repos-manager/log.sh`. `PREFIX` now defaults to `$(HOME)/.local`.

## v0.4.0

- Add per-provider help (`repos-manager github --help`)
- Add completions for all providers, commands and flags
- Fix `flake.nix` version is now kept in sync by the auto-bump workflow

## v0.3.0

- Add `--verbose` and `--quiet` flags for output control
- Add lockfile to prevent concurrent syncs on the same base directory
- Add `make lint`, `make test` and `make check` targets
- Add multi-OS CI tests (Linux + macOS)
- Add automated GitHub release on tag push
- Fix env vars (`REPOS_MANAGER_BASE_DIR`, `REPOS_MANAGER_PARALLEL`, `REPOS_MANAGER_PROTOCOL`) now override config file values
- Fix `repos-manager update` aborts on dirty repo instead of auto-stashing
- Add completions to auto-generated sourceme files
- Add auto version bump and tag on push to main
- Secure config directory with chmod 700
- Pin GitHub Actions to SHA for supply chain security

## v0.2.0

- Add `repos-manager status` command (dirty, ahead, behind, diverged)
- Add parallel sync with `--parallel` flag (default: 4 jobs)
- Add `repos-manager login` without provider (login all detected CLIs)
- Add `repos-manager init` to generate default config file
- Add `repos-manager update` for self-update
- Add Bitbucket provider (bitbucket-cli or REST API fallback)
- Add Radicle provider (rad CLI, peer-to-peer)
- Add config file support (`~/.config/repos-manager/config.json`)
- Add auto-generated sourceme files per host directory
- Add Makefile for simple install (`make install`)
- Add bats test suite (60 tests)
- Add GitHub Actions: tests, shellcheck, link checker
- Add documentation site (Eleventy)

## v0.1.0

- Initial release
- GitHub, GitLab, Forgejo/Gitea providers
- Clone and sync repos with namespace mirroring
- SSH and HTTPS support
- Filter by owner or repo (`--filter`)
- Exclude repos via `.repos-ignore`
- Include repos via `.repos-filter`
- Remove stale local repos (`--prune`)
- Preview mode (`--dry-run`)
- Self-hosted support (`--host`)
- Shell completions (bash, zsh, fish)
- NO_COLOR support
- Nix flake
