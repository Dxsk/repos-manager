---
title: Providers
description: Detailed setup for each supported Git provider.
order: 3
---

## Provider status

| Provider | Status | Notes |
|----------|--------|-------|
| GitHub | Tested | Stable, daily use, GitHub Enterprise via `--host` |
| GitLab | Tested | Stable, including self-hosted |
| Forgejo / Gitea | Tested | Stable |
| Bitbucket | Beta | API fallback implemented, needs more testing |
| Radicle | Beta | Experimental, peer-to-peer |

Bitbucket and Radicle support is functional but has not been extensively tested in production. If you encounter a bug, please [open an issue](https://forge.infrasouveraine.fr/dxsk/repos-manager/issues). Contributions and bug reports are welcome.

repos-manager drives each provider's official CLI, which must be in your `PATH`. `repos-manager login` without argument runs the login flow of every CLI it finds.

---

## GitHub

**CLI:** [`gh`](https://cli.github.com/) ([docs](https://cli.github.com/manual/))

```bash
# Install
sudo pacman -S github-cli      # Arch
brew install gh                # macOS
winget install GitHub.cli      # Windows

# Authenticate
repos-manager github login

# Sync
repos-manager github sync
```

Listing calls `gh api --paginate "/user/repos?affiliation=owner,collaborator,organization_member"`, so it covers your own repos, the repos of organizations you belong to, and repos on other personal accounts where you are a collaborator.

For GitHub Enterprise, run `gh auth login --hostname <host>` and add the host to `hosts.github` (or pass `--host`).

## GitLab

**CLI:** [`glab`](https://gitlab.com/gitlab-org/cli) ([docs](https://gitlab.com/gitlab-org/cli/-/blob/main/README.md))

```bash
# Install
sudo pacman -S glab        # Arch
brew install glab          # macOS
winget install GLab.GLab   # Windows

# Self-hosted
glab auth login --hostname gitlab.company.com
repos-manager gitlab sync --host gitlab.company.com
```

Lists every project you are a member of (`projects?membership=true`), with pagination for large instances. Groups and subgroups are mirrored as nested directories. Works with GitLab.com and self-hosted.

## Forgejo / Gitea

**CLI:** [`tea`](https://gitea.com/gitea/tea) ([Forgejo docs](https://forgejo.org/docs/latest/))

`gitea` is accepted as an alias: `repos-manager gitea sync` is the same as `repos-manager forgejo sync`.

```bash
# Install tea for authentication
sudo pacman -S tea   # Arch
brew install tea     # macOS

# Authenticate once per instance
tea login add
# or
repos-manager forgejo login

# Self-hosted
repos-manager forgejo sync --host git.example.com
```

### How listing works

`tea repo list` only returns the authenticated user's own repositories and does not enumerate organizations, so `repos-manager` talks to the Forgejo REST API directly instead. It reads the per-host URL and token from tea's config file, then paginates three endpoints and merges the results:

- `/api/v1/user/repos` for your personal repos and collaborations
- `/api/v1/user/orgs` for the organizations you belong to
- `/api/v1/orgs/{org}/repos` for every such organization

`tea` is only used to create the login entry; it is not called during sync. The config file is the source of truth for credentials.

### Where tea's config is read from

The first existing file wins:

1. `$TEA_CONFIG`, when set (and then only that file)
2. `$XDG_CONFIG_HOME/tea/config.yml`, when `XDG_CONFIG_HOME` is set
3. `~/.config/tea/config.yml`
4. The OS default location used by tea:

| OS | Path |
|----|------|
| Linux | `~/.config/tea/config.yml` |
| macOS | `~/Library/Application Support/tea/config.yml` |
| Windows | `%LOCALAPPDATA%\tea\config.yml` |

The login whose URL host matches the synced host (port included, e.g. `git.example.org:3000`) is used. If no `tea` login matches a configured host, the host is skipped with a warning and the sync continues with the next one.

## Bitbucket

**CLI:** [`bitbucket`](https://crates.io/crates/bitbucket-cli) or API fallback ([docs](https://developer.atlassian.com/cloud/bitbucket/rest/intro/))

If the `bitbucket` CLI is not installed, repos-manager falls back to the Bitbucket REST API with app password authentication.

```bash
# Authenticate (creates ~/.config/repos-manager/bitbucket-creds)
repos-manager bitbucket login

# Sync
repos-manager bitbucket sync
```

Create an app password at [bitbucket.org/account/settings/app-passwords](https://bitbucket.org/account/settings/app-passwords/) with `Repositories: Read` scope. With the API fallback, Bitbucket is also picked up by `sync --all` as soon as the credentials file exists.

## Radicle

**CLI:** [`rad`](https://radicle.xyz/guides/user) ([docs](https://radicle.xyz/guides/user))

Radicle is a sovereign peer-to-peer code forge. repos-manager lists the repos known to your local Radicle node (`rad ls --json`) and clones them under `<base_dir>/radicle/`.

```bash
# Install rad from https://radicle.xyz
# Authenticate (runs rad auth)
repos-manager radicle login

# Sync
repos-manager radicle sync
```
