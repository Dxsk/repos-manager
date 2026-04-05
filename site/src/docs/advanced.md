---
title: Advanced
description: Parallel tuning, self-update, sourceme, Nix, and dotenv integration.
order: 6
---

## Parallel sync

By default, repos-manager syncs 4 repos at a time. Adjust with:

```bash
repos-manager sync --all --parallel 8
```

Or set it in your config file:

{% raw %}
```json
{ "parallel": 8 }
```
{% endraw %}

For large organizations (100+ repos), higher parallelism helps. For slow connections, lower it to avoid timeouts.

## Concurrent sync protection

repos-manager uses a lockfile (`BASE_DIR/.repos-manager.lock`) to prevent multiple syncs from running at the same time on the same directory. If a previous sync was interrupted, the lockfile is automatically cleaned up when the holding process is no longer running.

## Self-update

repos-manager can update itself:

```bash
repos-manager update
```

If running from a git clone, it fetches and fast-forward pulls from `origin/main`. If installed via `make install`, it clones the latest version to a temp directory and reinstalls.

## Auto-generated sourceme files

After each sync, repos-manager generates `sourceme`, `sourceme.zsh`, and `sourceme.fish` files in each host directory. These provide the `repos-manager` function and full tab completions (providers, commands, flags) when you `cd` into the directory.

Combined with a [sourceme auto-loader](https://github.com/Dxsk/dotenv), the function loads and unloads automatically as you navigate your workspace.

## Security

The config directory (`~/.config/repos-manager/`) is created with `chmod 700` (owner-only access). Bitbucket API credentials stored in `bitbucket-creds` are set to `chmod 600`.

## Nix flake

repos-manager ships as a Nix flake for reproducible environments:

```bash
# Run directly
nix run github:Dxsk/repos-manager -- github sync

# Use as a template workspace
mkdir ~/my-repos && cd ~/my-repos
nix flake init -t github:Dxsk/repos-manager
nix develop
```

The dev shell provides `repos-manager` with all dependencies and sets `BASE_DIR` to the current directory.

## Dotenv integration

If you use [Dxsk/dotenv](https://github.com/Dxsk/dotenv) (GNU Stow-based dotfiles), repos-manager integrates via `scripts/projects.conf`:

```bash
git@github.com:Dxsk/repos-manager.git  make install
```

The dotenv install script clones and installs repos-manager automatically on new machines.
