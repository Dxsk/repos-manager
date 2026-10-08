# Contributing

Thanks for your interest in contributing to repos-manager!

The primary repository is on the Forgejo forge: https://forge.infrasouveraine.fr/dxsk/repos-manager. Issues and pull requests go there. GitHub (https://github.com/Dxsk/repos-manager) is a read-only mirror of `main` and tags.

For the full contributing guide (setup, testing, adding providers, PR rules), see the documentation:

https://repos-manager.dxscloud.fr/docs/contributing/

## Quick start

You need a stable [Rust toolchain](https://rustup.rs) and `git` 2.28 or newer.

```bash
git clone https://forge.infrasouveraine.fr/dxsk/repos-manager.git
cd repos-manager
git switch develop

cargo test                                   # Run tests
cargo fmt --check                            # Formatting
cargo clippy --all-targets -- -D warnings    # Lint
```

Branch from `develop`, one feature per PR, tests and lints must pass. Open the pull request on the [forge](https://forge.infrasouveraine.fr/dxsk/repos-manager/pulls).
