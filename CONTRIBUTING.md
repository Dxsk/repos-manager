# Contributing

Thanks for your interest in contributing to repos-manager!

The primary repository is on the Forgejo forge: https://forge.infrasouveraine.fr/dxsk/repos-manager. Issues and pull requests go there. GitHub (https://github.com/Dxsk/repos-manager) is a read-only mirror of `main` and tags.

The full guide (setup, testing, adding a provider, CI, releases) is on the documentation site: https://repos-manager.dxscloud.fr/docs/contributing/

## Quick start

You need a stable [Rust toolchain](https://rustup.rs), `make` and `git` 2.28 or newer.

```bash
git clone ssh://git@git.infrasouveraine.fr/dxsk/repos-manager.git
cd repos-manager
git switch develop

make build      # optimized binary in target/release/
make test       # unit and integration tests
make coverage   # tests plus line coverage (needs cargo-llvm-cov)
make lint       # rustfmt and clippy, as enforced by CI
```

## Pull requests

- Branch from `develop`, one feature per PR.
- `make test` and `make lint` must pass.
- Add your change under `## Unreleased` in [CHANGELOG.md](CHANGELOG.md) (`### Added`, `### Changed`, `### Removed` or `### Fixed`).
- Use Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`) and do not bump the version: it only changes through `make release`.

Open the pull request on the [forge](https://forge.infrasouveraine.fr/dxsk/repos-manager/pulls).
