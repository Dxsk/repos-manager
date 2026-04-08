# Changelog

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
