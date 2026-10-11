# Changelog

## 1.1.1 (2026-10-12)

### Fixed

- A git failure spanning several lines, such as an SSH or server error, broke the one line per repository report and ended on git's advice to check your access rights, which pointed at a permission problem that was often not the cause. The result line keeps what the server said, on one line
- `--prune --dry-run` ended on `Pruned 2 repositories` although it had deleted nothing. It says `Would prune` now, and counts one repository in the singular

## 1.1.0 (2026-10-12)

### Added

- Sourcehut provider (`sourcehut`, alias `srht`), listed through `hut graphql git`. Names keep their tilde (`~user/repo`), so quote a `--filter` pattern. **Alpha**
- OneDev provider (`onedev`), self-hosted only: a token per host, listing through `/~api/projects`, SSH on port 6611. Projects with code management disabled are skipped. **Alpha**
- `any` provider (alias `git`) for servers with no forge API: clone URLs from the new `repos` config key, or SSH discovery through gitolite, Soft Serve or `find`. A `file://` mirror works too
- The three `any` discovery modes are **alpha**: no live gitolite, Soft Serve or shell account has answered one yet
- Bitbucket Data Center and Server: any host but `bitbucket.org` is listed through the 1.0 API with an HTTP access token, Bearer when no user is stored. **Alpha**
- `gogs` is an alias of `forgejo` wherever `gitea` is, and `repos-manager forgejo login` stores a token without `tea`, which is what makes a Gogs server usable
- GitLab reports repository sizes, through `statistics=true`. Measured on 38 projects: 205 KB against 34 KB, 2.1 s against 1.1 s. Set `gitlab_statistics: false` for the light listing
- A size no provider reports is unknown, never 0 and never estimated. Bitbucket Data Center, OneDev, Sourcehut, Radicle and `any` report none
- Reconnaissance before a sync downloads anything: per host, how many repositories are in scope, cloned and new, the volume of the new ones and the two biggest, then one `Proceed? [Y/n]` for the whole run
- It only speaks above a threshold, by default 5 new clones or 1 GB across the run, so a daily sync where nothing is new prints what it printed before
- The question needs stdin and stdout to both be terminals, so cron and CI get the summary and carry on. So do `--dry-run` and the new `-y` / `--yes`
- Answering `n` exits successfully and names the ways to sync less. A host whose listing came back short says so before the question
- The volume is the repository size the forge reports, measured on the server and not on the wire, hence "about"
- `confirm_sync` (`auto`, `always`, `never`), `confirm_new_repos` and `confirm_download_gb` in config.json set all of that per machine
- `sync --plan` prints the summary and stops. Unlike `--dry-run` it reports the aggregate only, and it always prints, even under `-q`
- `sync --write-ignore` writes `<base_dir>/.repos.yml` holding every repository in scope, biggest first, all commented out next to its size. Uncomment what you do not want, then sync
- The generated file changes nothing as written, and only ever holds `ignore` lists, so it cannot empty your scope. Overwriting it needs `--force`, which carries the existing rules over
- `--force` now serves `--prune` and `--write-ignore`, and still means nothing on its own

### Changed

- **Filtering moved to a single `<base_dir>/.repos.yml`.** `only:` allows and `ignore:` denies, both globally and under `hosts.<host>:`, so one workspace can hold different rules per host
- Allow-lists stack and a repository must pass every one; deny-lists add up. A host block can only narrow: it can take a repository out of the reach of `--prune`, never pull one in
- The file is read strictly, because it decides what `--prune` deletes: an unknown key, malformed YAML or an unreadable file stops the run before any host syncs
- An `only: []` warns instead of emptying the scope, and a host left with nothing in scope names both allow-lists, so it cannot be mistaken for an empty remote
- `--filter` is repeatable and reads as one OR-list, and a new repeatable `--ignore <pattern>` does the opposite
- `.repos-filter` and `.repos-ignore` keep working as in 1.0 while there is no `.repos.yml`. Once it exists they are ignored, with a warning
- **Bitbucket credentials changed shape.** Atlassian removed app passwords on 2026-07-28, so Cloud now needs an Atlassian API token, sent as Basic auth with the account email
- `bitbucket-creds` became a JSON map keyed by host, so Cloud and self-hosted instances coexist. An old `user:password` line is still read as `bitbucket.org`, with a warning. **Alpha**
- The Bitbucket Cloud listing now covers every workspace the user is a member of, instead of the single workspace named after their username
- `repos-manager onedev login` keeps the scheme it was given, so an instance served over plain HTTP can be synced. An entry saved without one still defaults to HTTPS
- All hosts of a sync fetch their repository lists concurrently, and their clones and updates share one queue, so `--parallel` bounds the total number of git processes
- Updates fetch once and fast-forward the branch they just fetched instead of running a second `git pull`, about twice as fast. Result lines say `updated` or `up to date`
- Jobs are scheduled rather than run in listing order: clones first, biggest first, and never more than 10 per host, which keeps a sync under the default `MaxStartups` of sshd
- Each result line shows a `[done/total]` counter, and an interactive terminal shows a live status line with the repositories in progress

### Fixed

- `--plan`, `--write-ignore` and `--dry-run` no longer create a host directory and a lock file for a host they only report on
- Paths are printed with one separator. A `base_dir` written with forward slashes in the config file used to show as `C:/dir\file` once joined
- A listing that lost entries was reported as complete, so `--prune` read the gap as a deleted remote and removed the clone. A provider now says when entries are missing, and that host is left alone
- A listing could never end on a server that ignores pagination: Gogs answers with the whole list whatever `page` says. A listing now stops when a page repeats the previous one, and warns
- A sync could hang forever on a dead connection or a prompt nobody could answer. SSH runs in batch mode with keepalives, and every git command and listing has a hard timeout (`REPOS_MANAGER_GIT_TIMEOUT`, `REPOS_MANAGER_LIST_TIMEOUT`)
- Repositories whose names collapse to one directory once the filesystem normalizes them, or that would nest inside another clone, are skipped with the reason. The clone already on disk keeps its directory
- Names Windows cannot hold (`aux`, `con`, a trailing dot) are skipped too, instead of `name.` and `name` sharing one clone
- Two targets sharing a host directory no longer fail with `another sync is already running`. They are listed together under one lock, and prune is skipped unless every listing succeeded
- `--prune` could destroy work that existed only locally. It now keeps any repo with uncommitted changes, unpushed commits or a stash, and stops when more than half of a host would go, unless `--force`
- `--prune` now works with `--filter`, pruning within the scope of the filter instead of being skipped
- A sync no longer moves the branch you are working on: `git pull origin HEAD` used to fast-forward whatever was checked out. Other branches are only fetched, and a renamed default branch is detected again
- Updating a clone of an empty remote no longer fails with `couldn't find remote ref HEAD`, it is reported as skipped
- On Windows, a repository tracking a file name Windows forbids is reported as skipped with the offending path, instead of a half-done clone skipped as dirty on every sync
- A failed or interrupted clone no longer leaves a half-done repository behind: clones land in a `.<name>.partial` sibling and move into place once complete

### Security

- Repository names from a provider API can no longer place a clone outside the base directory on Windows: backslash separators, `..` and drive prefixes such as `C:` are dropped like `/`-separated `..`

## 1.0.0 (2026-10-08)

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
- Packaging for crates.io (`cargo install repos-manager`) and the AUR (`repos-manager` built from source, `repos-manager-bin` prebuilt), rendered from a release by `packaging/aur/render.sh`
- `repos-manager update` and the update banner detect installs owned by a package manager (AUR or other `/usr/bin` packages, Homebrew, Scoop, winget, cargo) and point to its upgrade command instead of replacing the binary
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

- The Bash sources. A transitional `repos-manager.sh` stub stays at the repo root for a while: it only carries `VERSION`, so 0.x installs show the 1.0 update banner, and prints the upgrade steps when a git-clone install runs it
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
