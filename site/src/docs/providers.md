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

Fetches repos from your account and all organizations you belong to.

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

**CLI:** [`tea`](https://forgejo.org/docs/latest/user/cli/) ([docs](https://forgejo.org/docs/latest/))

```bash
# Install
sudo pacman -S tea  # Arch

# Self-hosted
repos-manager forgejo sync --host git.example.com
```

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
