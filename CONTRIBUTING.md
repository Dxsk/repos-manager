# Contributing

Thanks for your interest in contributing to repos-manager!

Issues and pull requests are handled on GitHub: https://github.com/Dxsk/repos-manager. Development happens upstream on the maintainer's Forgejo instance and is mirrored to GitHub, so an accepted pull request is landed upstream and shows up on GitHub through the mirror.

The full guide (setup, testing, adding a provider, CI) is on the documentation site: https://repos-manager.dxscloud.fr/docs/contributing/

## Quick start

You need a stable [Rust toolchain](https://rustup.rs) and `git` 2.28 or newer. `make` is optional, see below.

```bash
git clone https://github.com/Dxsk/repos-manager.git
cd repos-manager

make build      # optimized binary in target/release/
make test       # unit and integration tests
make coverage   # tests plus line coverage (needs cargo-llvm-cov)
make lint       # rustfmt and clippy, as enforced by CI
make metrics    # per-function complexity budgets (needs rust-code-analysis-cli)
make mutants    # mutation testing on sync and git (needs cargo-mutants, slow)
```

`make` is a convenience, not a requirement: every target is one command you can run directly, which is how the project is developed on Windows.

| Target | Direct command |
|--------|----------------|
| `make build` | `cargo build --release --locked` |
| `make test` | `cargo test --locked` |
| `make lint` | `cargo fmt --check` then `cargo clippy --all-targets --locked -- -D warnings` |
| `make metrics` | `python scripts/code-metrics.py` |
| `make coverage` | `cargo llvm-cov --locked --summary-only` |
| `make release V=x.y.z` | `python scripts/release.py x.y.z` |

The extra tools:

```bash
cargo install rust-code-analysis-cli --locked   # Mozilla's metrics analyser
cargo install cargo-mutants --locked
```

`make metrics` reads Mozilla's [rust-code-analysis](https://github.com/mozilla/rust-code-analysis) and fails when a function goes over the complexity budgets in [scripts/code-metrics.py](scripts/code-metrics.py). `make mutants` rewrites one expression at a time and checks that a test notices. A full run still reports about a dozen survivors in counters and printed lines, so read it as a report, not a gate. CI runs it only on the lines a pull request touches, where a survivor does mean the new code is untested. The ones that survive on purpose are listed, with a reason each, in [.cargo/mutants.toml](.cargo/mutants.toml).

Two environment variables exist for the tests only and are not part of the user interface: `REPOS_MANAGER_UPDATE_URL` (plus `_CACHE` and `_TTL`) points the update check at a local server, and `REPOS_MANAGER_FAKE_TTY=1` makes the sync confirmation behave as if stdin and stdout were terminals, since `cargo test` has none.

## Pull requests

- Fork the repository and branch from `main`, one feature per PR.
- `make test`, `make lint` and `make metrics` must pass.
- Add your change under `## Unreleased` in [CHANGELOG.md](CHANGELOG.md) (`### Added`, `### Changed`, `### Removed` or `### Fixed`).
- Use Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`) and do not bump the version: it only changes through [scripts/release.py](scripts/release.py).

Open the pull request on [GitHub](https://github.com/Dxsk/repos-manager/pulls).

## Security issues

Do not open a public issue for a vulnerability. See [SECURITY.md](SECURITY.md).
