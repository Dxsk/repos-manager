---
title: Providers
description: Detailed setup for each supported Git provider.
order: 3
---

## Provider status

| Provider | Status | Notes |
|----------|--------|-------|
| GitHub | Tested | Stable, daily use |
| GitLab | Tested | Stable, including self-hosted |
| Forgejo / Gitea | Tested | Stable |
| Bitbucket | Beta | API fallback implemented, needs more testing |
| Radicle | Beta | Experimental, peer-to-peer |

Bitbucket and Radicle support is functional but has not been extensively tested in production. If you encounter a bug, please [open an issue](https://github.com/Dxsk/repos-manager/issues) - contributions and bug reports are welcome.

---

## GitHub

**CLI:** [`gh`](https://cli.github.com/) ([docs](https://cli.github.com/manual/))

```bash
# Install
sudo pacman -S github-cli  # Arch
brew install gh             # macOS

# Authenticate
repos-manager github login

# Sync
repos-manager github sync
```

Fetches every repo you have access to: your own, repos in organizations you belong to, and repos where you are a plain collaborator on someone else's personal account. Listing hits `/user/repos?affiliation=owner,collaborator,organization_member` with `--paginate`, so collaborations on personal accounts are picked up without any per-org fan-out.

## GitLab

**CLI:** [`glab`](https://gitlab.com/gitlab-org/cli) ([docs](https://gitlab.com/gitlab-org/cli/-/blob/main/README.md))

```bash
# Install
sudo pacman -S glab  # Arch
brew install glab    # macOS

# Self-hosted
repos-manager gitlab sync --host gitlab.company.com
```

Supports pagination for large instances. Works with GitLab.com and self-hosted.

## Forgejo / Gitea

**CLI:** [`tea`](https://forgejo.org/docs/latest/) ([docs](https://forgejo.org/docs/latest/))
**Extra runtime deps:** `curl`, `yq`

```bash
# Install tea for authentication
sudo pacman -S tea curl yq   # Arch
brew install tea curl yq     # macOS

# Authenticate once per instance
tea login add
# or
repos-manager forgejo login

# Self-hosted
repos-manager forgejo sync --host git.example.com
```

### How listing works

`tea repo list` only returns the authenticated user's own repositories and does not enumerate organizations, so `repos-manager` talks to the Forgejo REST API directly instead. It reads the per-host URL and token from `~/.config/tea/config.yml`, then paginates three endpoints and merges the results:

- `/api/v1/user/repos` for your personal repos
- `/api/v1/user/orgs` for the organizations you belong to
- `/api/v1/orgs/{org}/repos` for every such organization

`tea` itself is still the recommended way to create the login entry, but it is not called during sync. The config file is the source of truth for credentials.

If no `tea` login matches a configured host, the host is skipped with a warning and the sync continues with the next one. If `curl` or `yq` is missing, sync aborts with a clear message telling you which package to install.

## Bitbucket

**CLI:** [`bitbucket`](https://crates.io/crates/bitbucket-cli) or API fallback ([docs](https://developer.atlassian.com/cloud/bitbucket/rest/intro/))

If the `bitbucket` CLI is not installed, repos-manager falls back to the Bitbucket REST API with app password authentication.

```bash
# Authenticate (creates ~/.config/repos-manager/bitbucket-creds)
repos-manager bitbucket login

# Sync
repos-manager bitbucket sync
```

Create an app password at [bitbucket.org/account/settings/app-passwords](https://bitbucket.org/account/settings/app-passwords/) with `Repositories: Read` scope.

## Radicle

**CLI:** [`rad`](https://radicle.xyz/guides/user) ([docs](https://radicle.xyz/guides/user))

Radicle is a sovereign peer-to-peer code forge. repos-manager lists tracked repos from your local Radicle node.

```bash
# Install rad from https://radicle.xyz
# Authenticate
repos-manager radicle login

# Sync
repos-manager radicle sync
```
