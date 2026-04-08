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

### Background update banner

You don't have to run `update` to know a new version is out. On every invocation (except `update`, `version` and `help`), repos-manager spawns a detached background `curl` that fetches the latest `VERSION` from `main` and caches it under `${XDG_CACHE_HOME:-~/.cache}/repos-manager/latest-version`. The cache has a 24h TTL and is compared against the running binary on the next run. When a newer release is available, a one-line yellow banner is printed on stderr before the command runs:

```bash
⬆ repos-manager 0.6.0 available (current 0.5.0) - run: repos-manager update
```

The check is non-blocking (the background job is detached with redirected fds), silent when nothing new is available or when `curl` is missing, and disabled when:

- `REPOS_MANAGER_NO_UPDATE_CHECK=1` is set in the environment, or
- `check_updates: false` is set in `~/.config/repos-manager/config.json`.

Tune the cache location with `REPOS_MANAGER_UPDATE_CACHE` and the refresh interval with `REPOS_MANAGER_UPDATE_TTL` (seconds).

## Fast status on large workspaces

`repos-manager status` is built to stay responsive even when `base_dir` contains hundreds of repos and large dependency trees. Three things make it fast:

- **Heavy directory pruning.** `find` skips `node_modules`, `.venv`, `venv`, `__pycache__`, `target`, `vendor`, `dist`, `build`, `.next` and `.cache` before descending, so vendored libraries never bloat the walk.
- **Network / FUSE mount skip.** `status` parses `/proc/self/mountinfo` and prunes every mount point under `base_dir` whose filesystem type is `fuse`, `fuse.*`, `nfs`, `cifs`, `smb*`, `smbfs`, `afs`, `ceph` or `davfs`. Cloud drives (kDrive, Dropbox, sshfs) are the number one cause of apparent hangs on `status`, and pruning them by default turns a multi-minute freeze into a sub-second scan. See [Configuration → Network-mount scanning](./configuration.md) to opt in if you really host repos on a reliable network share.
- **Streaming progress indicator.** Each repo found by `find` is inspected as it streams in (no `sort` buffering), and a `[N] provider/owner/repo` line is updated in place on stderr so you always see where the scan is. The indicator auto-disables when stderr is not a TTY or when `--quiet` is set.

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
