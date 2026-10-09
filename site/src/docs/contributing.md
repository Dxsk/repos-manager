---
title: Contributing
description: "Contribute to repos-manager: build and test the Rust code, add a provider, follow the release process and open pull requests on GitHub."
order: 7
---

## Where to contribute

Issues and pull requests are handled on [GitHub](https://github.com/Dxsk/repos-manager). Development happens upstream on the maintainer's Forgejo instance and is mirrored to GitHub, so an accepted pull request is landed upstream and appears on GitHub through the mirror.

Security issues are reported privately through a [GitHub security advisory](https://github.com/Dxsk/repos-manager/security/advisories/new), never in a public issue.

## Setup

You need a recent stable [Rust toolchain](https://rustup.rs) (with `rustfmt` and `clippy`), `make` and `git` 2.28 or newer.

```bash
git clone https://github.com/Dxsk/repos-manager.git
cd repos-manager
make build
```

Run the CLI without installing it:

```bash
cargo run -- github sync --dry-run --base-dir /tmp/rm
```

## Make targets

The root `Makefile` wraps the usual commands:

| Target | What it does |
|--------|--------------|
| `make build` | `cargo build --release --locked`, binary in `target/release/` |
| `make test` | Unit and integration tests |
| `make coverage` | Tests plus a line coverage summary (needs `cargo install cargo-llvm-cov`) |
| `make lint` | `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` |
| `make install` / `make uninstall` | Install into `$(PREFIX)/bin` (`PREFIX ?= ~/.local`, `DESTDIR` supported) |
| `make completions` | bash, zsh and fish completions in your user directories |
| `make release V=x.y.z` | Cut a release, see below |

## Running tests

```bash
make test                                         # everything
cargo test sync::tests::                          # one module
cargo test -- --exact matcher::tests::exact_and_glob
```

Unit tests live next to the code in each module (`#[cfg(test)] mod tests`), and `tests/cli.rs` runs the compiled binary end to end. Tests shell out to the real `git` and create bare remotes in temporary directories, which is why `git init -b` support (git 2.28+) is required.

`make coverage` prints per-file line coverage. The Forgejo CI runs it on every push, so you can read the figures in the job log.

When running the binary from scripts, set `NO_COLOR=1` and `REPOS_MANAGER_NO_UPDATE_CHECK=1` to get plain output and no background network call.

## Formatting and linting

```bash
cargo fmt
make lint
```

CI runs `cargo fmt --check` and clippy with warnings as errors, so run `make lint` before pushing.

## CI pipeline

GitHub runs CI on every pull request and on `main`. The upstream forge runs its own checks before anything is mirrored:

| Where | Job | What it does |
|-------|-----|-------------|
| Forgejo Actions | CI | `cargo fmt --check`, `cargo clippy -D warnings`, ShellCheck on `installers/install.sh`, tests and coverage on Linux |
| GitHub Actions | CI | Format, lint and ShellCheck, then tests on Ubuntu, macOS and Windows |
| GitHub Actions | Links | Lychee validates URLs in the markdown files |

All checks must pass before merging.

## Releases

Releases are cut by the maintainer, by hand, from `main` on the upstream forge. There is no automatic version bump.

1. Describe your changes under `## Unreleased` in `CHANGELOG.md`, grouped as `### Added`, `### Changed`, `### Removed` and `### Fixed`. Contributors do this in their pull request.
2. The maintainer runs `make release V=x.y.z` on a clean tree. It sets the version in `Cargo.toml`, `Cargo.lock` and `site/src/_data/site.json`, renames `## Unreleased` to `## x.y.z (YYYY-MM-DD)`, commits `chore: release vx.y.z` and creates the signed `vx.y.z` tag (`git tag -s`, so a GPG signing key is needed). Nothing is pushed yet.
3. `git push --follow-tags` sends the commit and tag to the forge.

The tag starts the Forgejo release (Linux x86_64 and aarch64 musl, `x86_64-pc-windows-gnu` zip and the x86_64 installer). The forge push mirror then forwards `main` and the tag to GitHub, which builds the full release: Linux, macOS, Windows x86_64 and ARM64 (`msvc`) and both installers. Both releases ship `SHA256SUMS` and `SHA256SUMS-binaries`. The install script and `repos-manager update` download from GitHub, so its release must carry every asset.

Once the GitHub release is complete, publish the packages:

```bash
cargo publish                                # crates.io (needs `cargo login`)
packaging/aur/render.sh x.y.z --publish      # AUR repos-manager and repos-manager-bin (needs your AUR SSH key)
```

`render.sh` without `--publish` only writes `PKGBUILD` and `.SRCINFO` to `dist/aur/` for review.

Since the mirror overwrites GitHub `main` on every sync, pull requests are never merged with the GitHub button: the maintainer lands them upstream (keeping the author) and the mirror closes the loop.

## Adding a provider

Providers live in `src/providers/`, one file each. A provider lists remote repos as a `Vec<Repo>` (`full_name`, `ssh_url`, `https_url`) and knows how to log in.

### Step 1: Create the provider file

Create `src/providers/yourprovider.rs`:

```rust
use std::process::Command;

use anyhow::Result;
use serde_json::Value;

use super::{ListError, Repo, require_cli, run_capture, run_interactive, str_field};

pub fn login() -> Result<()> {
    run_interactive("yourcli", &["auth", "login"])
}

/// Map the provider payload to `Repo`. Kept separate from I/O so it can be unit tested.
pub fn parse(items: &[Value]) -> Vec<Repo> {
    items
        .iter()
        .map(|r| Repo {
            full_name: str_field(r, "full_name"),
            ssh_url: str_field(r, "ssh_url"),
            https_url: str_field(r, "https_url"),
        })
        .filter(|r| !r.full_name.is_empty())
        .collect()
}

pub fn list_repos(host: &str) -> Result<Vec<Repo>, ListError> {
    require_cli("yourcli")?;
    let raw = run_capture(Command::new("yourcli").args(["repo", "list", "--json", "--host", host]))?;
    let items: Vec<Value> = serde_json::from_str(&raw).map_err(anyhow::Error::from)?;
    Ok(parse(&items))
}
```

Return `ListError::Skip(reason)` when the host is simply not configured (no login, no credentials): the sync warns and moves on. Any other error is a `ListError::Fail`. If the CLI paginates by printing several JSON arrays back to back, use `parse_paginated` from `mod.rs`.

### Step 2: Register it

In `src/providers/mod.rs`:

1. Declare the module: `mod yourprovider;`
2. Add a `YourProvider` variant to the `Provider` enum and to `Provider::ALL`
3. Fill in its arms in `name()`, `from_name()`, `cli()`, `default_host()`, `login()` and `list_repos()`

In `src/cli.rs`:

4. Add a `YourProvider(ProviderCmd)` variant to `Command` (its doc comment becomes the help text)
5. Add a `YourProvider` variant to `ProviderArg` and to the `From<ProviderArg> for Provider` impl

In `src/main.rs`:

6. Dispatch it in `run()`: `Command::YourProvider(p) => provider_cmd(Provider::YourProvider, p.action, settings)`

The compiler points at every `match` you missed.

### Step 3: Add tests

At the bottom of the provider file, test the payload parser with a sample response:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_payload() {
        let items: Vec<Value> = serde_json::from_str(
            r#"[{"full_name":"user/repo","ssh_url":"git@host:user/repo.git","https_url":"https://host/user/repo.git"}]"#,
        )
        .unwrap();
        let r = &parse(&items)[0];
        assert_eq!(r.full_name, "user/repo");
        assert_eq!(r.ssh_url, "git@host:user/repo.git");
    }
}
```

### Step 4: Document

Update `readme.md`, `site/src/docs/providers.md`, the default hosts table in `site/src/docs/configuration.md` and the glossary.

## Project structure

<pre style="background:var(--bg-secondary);border:1px solid var(--border);border-radius:8px;padding:1.2em;overflow-x:auto;font-size:0.85em;line-height:1.6"><code><span style="color:#3fb950">$</span> <span style="color:#58a6ff">tree</span> repos-manager/
<span style="color:#58a6ff">repos-manager/</span>
├── <span style="color:#d29922">Cargo.toml</span>               <span style="color:#8b949e"># Crate manifest</span>
├── <span style="color:#d29922">Makefile</span>                 <span style="color:#8b949e"># build, test, coverage, lint, install, release</span>
├── <span style="color:#58a6ff">src/</span>
│   ├── <span style="color:#3fb950">main.rs</span>              <span style="color:#8b949e"># Entry point, command dispatch</span>
│   ├── <span style="color:#3fb950">cli.rs</span>               <span style="color:#8b949e"># clap command tree and flags</span>
│   ├── <span style="color:#3fb950">config.rs</span>            <span style="color:#8b949e"># Settings: flags, env vars, config.json</span>
│   ├── <span style="color:#3fb950">matcher.rs</span>           <span style="color:#8b949e"># --filter, .repos-filter, .repos-ignore</span>
│   ├── <span style="color:#3fb950">sync.rs</span>              <span style="color:#8b949e"># Per-host lock, parallel sync, prune</span>
│   ├── <span style="color:#3fb950">status.rs</span>            <span style="color:#8b949e"># Status scan (network-mount aware)</span>
│   ├── <span style="color:#3fb950">update.rs</span>            <span style="color:#8b949e"># Update banner and self-update</span>
│   ├── <span style="color:#3fb950">git.rs</span>               <span style="color:#8b949e"># git command helpers</span>
│   ├── <span style="color:#3fb950">http.rs</span>              <span style="color:#8b949e"># HTTP client</span>
│   ├── <span style="color:#3fb950">output.rs</span>            <span style="color:#8b949e"># Colors, log levels</span>
│   └── <span style="color:#58a6ff">providers/</span>
│       ├── <span style="color:#3fb950">mod.rs</span>           <span style="color:#8b949e"># Provider enum, shared helpers</span>
│       ├── <span style="color:#3fb950">github.rs</span>        <span style="color:#8b949e"># GitHub (gh)</span>
│       ├── <span style="color:#3fb950">gitlab.rs</span>        <span style="color:#8b949e"># GitLab (glab)</span>
│       ├── <span style="color:#3fb950">forgejo.rs</span>       <span style="color:#8b949e"># Forgejo/Gitea (tea config + REST API)</span>
│       ├── <span style="color:#3fb950">bitbucket.rs</span>     <span style="color:#8b949e"># Bitbucket (CLI or REST API)</span>
│       └── <span style="color:#3fb950">radicle.rs</span>       <span style="color:#8b949e"># Radicle (rad)</span>
├── <span style="color:#58a6ff">tests/</span>
│   └── <span style="color:#3fb950">cli.rs</span>               <span style="color:#8b949e"># End-to-end CLI tests</span>
├── <span style="color:#58a6ff">installers/</span>              <span style="color:#8b949e"># install.sh, install.ps1, NSIS installer</span>
└── <span style="color:#58a6ff">site/</span>                    <span style="color:#8b949e"># This documentation site (Eleventy)</span></code></pre>

## Pull requests

| Rule | Details |
|------|---------|
| Where | Pull requests on [GitHub](https://github.com/Dxsk/repos-manager/pulls), from a fork |
| Branch from | `main` |
| Scope | One feature per PR |
| Tests | Must pass (`make test`) |
| Lint | Must pass (`make lint`) |
| Changelog | Add your entry under `## Unreleased` in `CHANGELOG.md` |
| Commits | Conventional style (`feat:`, `fix:`, `docs:`, `test:`) |

Do not bump the version in your pull request: it only changes through `make release`.
