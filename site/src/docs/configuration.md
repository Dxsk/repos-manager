---
title: Configuration
description: "Configure repos-manager with config.json and environment variables: base directory, protocol, parallel jobs and several hosts per provider."
order: 2
---

## Config file

Generate a default config:

```bash
repos-manager init
```

This creates `~/.config/repos-manager/config.json`. The path is the same on every OS, including Windows (`%USERPROFILE%\.config\repos-manager\config.json`), and can be overridden with `REPOS_MANAGER_CONFIG`.

{% raw %}
```json
{
  "base_dir": "~/Documents",
  "parallel": 4,
  "protocol": "ssh",
  "check_updates": true,
  "scan_network_mounts": false,
  "hosts": {
    "github":    ["github.com"],
    "gitlab":    ["gitlab.com"],
    "forgejo":   ["codeberg.org"],
    "bitbucket": ["bitbucket.org"]
  }
}
```
{% endraw %}

| Key | Description | Default |
|-----|-------------|---------|
| `base_dir` | Root directory for all repos (`~` is expanded, must resolve to an absolute path) | `~/Documents` |
| `parallel` | Number of parallel sync jobs | `4` |
| `protocol` | Clone protocol (`ssh` or `https`) | `ssh` |
| `check_updates` | Check for a newer release and show a banner | `true` |
| `scan_network_mounts` | Let `status` traverse FUSE, NFS or SMB mounts under `base_dir` (see warning below) | `false` |
| `hosts.<provider>` | List of hosts to sync for that provider | See defaults below |

An invalid JSON file is reported as an error instead of being silently ignored.

### Update banner

When `check_updates` is enabled, `login`, `sync` and `status` read a cached "latest version" and print a single-line yellow banner when it is newer than the running binary. If the cache is older than 24 hours, a detached background copy of repos-manager refreshes it from the GitHub Releases API. The check never blocks the command in progress and stays silent when nothing new is available or the network is down.

Opt out at any time without editing the config file:

```bash
REPOS_MANAGER_NO_UPDATE_CHECK=1 repos-manager status
```

### Network-mount scanning

`repos-manager status` walks `base_dir` recursively to collect every `.git` directory. Cloud drives mounted via FUSE (kDrive, Dropbox, sshfs, and similar) and network filesystems (NFS, SMB/CIFS, davfs, Ceph, AFS) turn each readdir into a blocking round-trip, and a single repo on such a mount can hang the scan for minutes.

On Linux, `status` reads `/proc/self/mountinfo` by default, finds every mount point under `base_dir` whose filesystem type matches `fuse`, `fuse.*`, `nfs`, `cifs`, `smb*`, `smbfs`, `afs`, `ceph` or `davfs`, and prunes it from the walk. Use `--verbose` to see which mounts were skipped. On macOS and Windows there is no such detection.

Set `scan_network_mounts: true` only if you really do host working copies on a reliable network share. The scan will then descend into those mounts and print a warning that this can cause long hangs on flaky links.

### Multiple hosts per provider

Each provider key under `hosts` accepts an **array** of hostnames, so you can sync several GitHub, GitLab or Forgejo instances side-by-side (typical setup: a SaaS one + your self-hosted one).

{% raw %}
```json
{
  "hosts": {
    "gitlab":  ["gitlab.com", "gitlab.example.org"],
    "forgejo": ["codeberg.org", "forge.example.org"]
  }
}
```
{% endraw %}

`repos-manager gitlab sync` and `repos-manager sync --all` will then iterate over every configured host. Repositories are mirrored under `<base_dir>/<host>/...`, so each instance gets its own directory tree and its own sync lock.

The legacy string form (`"gitlab": "gitlab.com"`) is still accepted and treated as a single-element list. `gitea` is accepted as an alias of `forgejo`.

**Per-CLI requirements for self-hosted instances:**

- **GitHub Enterprise**: `repos-manager` passes `--hostname <host>` to `gh`. Run `gh auth login --hostname <host>` once per host.
- **GitLab**: `repos-manager` sets `GITLAB_HOST` so `glab` targets the right instance. Run `glab auth login --hostname <host>` once per host.
- **Forgejo / Gitea**: `repos-manager` reads the matching login entry (URL + token) from tea's config file and talks to the Forgejo REST API directly. Run `tea login add` once per instance. If no matching login is found, the host is **skipped with a warning** and the sync continues with the next one.
- **Bitbucket / Radicle**: the host is only used for the directory name; listing always targets Bitbucket Cloud or your local Radicle node.

You can still override the configured list for a one-off sync with `repos-manager <provider> sync --host <hostname>`.

### Defaults

If `hosts` is missing or empty for a provider, `repos-manager` falls back to the SaaS default:

| Provider | Default host |
|----------|--------------|
| `github` | `github.com` |
| `gitlab` | `gitlab.com` |
| `forgejo` | `codeberg.org` |
| `bitbucket` | `bitbucket.org` |
| `radicle` | `radicle` (local node) |

## Environment variables

| Variable | Description | Default |
|----------|-------------|---------|
| `REPOS_MANAGER_CONFIG` | Path to config file | `~/.config/repos-manager/config.json` |
| `REPOS_MANAGER_BASE_DIR` | Override base directory | `~/Documents` |
| `REPOS_MANAGER_PARALLEL` | Override parallel jobs | `4` |
| `REPOS_MANAGER_PROTOCOL` | Override protocol (`ssh` or `https`) | `ssh` |
| `REPOS_MANAGER_NO_UPDATE_CHECK` | Set to `1` to skip the background update check | Unset |
| `REPOS_MANAGER_UPDATE_TTL` | Seconds between update checks (cache TTL) | `86400` |
| `REPOS_MANAGER_UPDATE_CACHE` | Path to the cached latest-version file | `<cache dir>/repos-manager/latest-version` |
| `REPOS_MANAGER_UPDATE_URL` | Release API endpoint used by the update check and `update` | GitHub Releases API |
| `TEA_CONFIG` | Path to tea's config file, read by the Forgejo provider | tea's default location (see Providers) |
| `NO_COLOR` | Disable colored output | Unset |

The cache directory is `$XDG_CACHE_HOME` when set, otherwise the OS cache directory (`~/.cache` on Linux, `~/Library/Caches` on macOS, `%LOCALAPPDATA%` on Windows).

Environment variables take precedence over the config file. Flags take precedence over both.
