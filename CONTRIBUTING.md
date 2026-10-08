# Contributing

Thanks for your interest in contributing to repos-manager!

Issues and pull requests are handled on GitHub: https://github.com/Dxsk/repos-manager. Development happens upstream on the maintainer's Forgejo instance and is mirrored to GitHub, so an accepted pull request is landed upstream and shows up on GitHub through the mirror.

The full guide (setup, testing, adding a provider, CI) is on the documentation site: https://repos-manager.dxscloud.fr/docs/contributing/

## Quick start

You need a stable [Rust toolchain](https://rustup.rs), `make` and `git` 2.28 or newer.

```bash
git clone https://github.com/Dxsk/repos-manager.git
cd repos-manager

make build      # optimized binary in target/release/
make test       # unit and integration tests
make coverage   # tests plus line coverage (needs cargo-llvm-cov)
make lint       # rustfmt and clippy, as enforced by CI
```

## Pull requests

- Fork the repository and branch from `main`, one feature per PR.
- `make test` and `make lint` must pass.
- Add your change under `## Unreleased` in [CHANGELOG.md](CHANGELOG.md) (`### Added`, `### Changed`, `### Removed` or `### Fixed`).
- Use Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`) and do not bump the version: it only changes through `make release`.

Open the pull request on [GitHub](https://github.com/Dxsk/repos-manager/pulls).

## Security issues

Do not open a public issue for a vulnerability. See [SECURITY.md](SECURITY.md).
