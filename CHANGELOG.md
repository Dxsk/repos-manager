# Changelog

## v0.5.0

- Add multi-host support per provider (#3)

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
