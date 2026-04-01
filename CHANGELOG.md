# Changelog

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
