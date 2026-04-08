---
title: Configuration
description: Configure repos-manager with a config file, environment variables, and custom hosts.
order: 2
---

## Config file

Generate a default config:

```bash
repos-manager init
```

This creates `~/.config/repos-manager/config.json`:

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
| `base_dir` | Root directory for all repos | `~/Documents` |
| `parallel` | Number of parallel sync jobs | `4` |
| `protocol` | Clone protocol (`ssh` or `https`) | `ssh` |
| `check_updates` | Check GitHub for a newer release and show a banner | `true` |
| `scan_network_mounts` | Let `status` traverse FUSE, NFS or SMB mounts under `base_dir` (see warning below) | `false` |
| `hosts.<provider>` | List of hosts to sync for that provider | See defaults below |

### Update banner

When `check_updates` is enabled, every invocation (except `update`, `version` and `help`) spawns a detached background `curl` that fetches the latest `VERSION` from `main` and caches it under `~/.cache/repos-manager/latest-version` for 24 hours. The next run compares the cached value against the running binary and prints a single-line yellow banner when a newer release is available. The check never blocks the command in progress and is silent when nothing is available, when `curl` is missing, or when opted out.

Opt out at any time without editing the config file:

```bash
REPOS_MANAGER_NO_UPDATE_CHECK=1 repos-manager status
```

### Network-mount scanning

`repos-manager status` walks `base_dir` recursively to collect every `.git` directory. Cloud drives mounted via FUSE (kDrive, Dropbox, sshfs, and similar) and network filesystems (NFS, SMB/CIFS, davfs, Ceph, AFS) turn each readdir into a blocking round-trip, and a single repo on such a mount can hang the scan for minutes.

By default, `status` reads `/proc/self/mountinfo`, finds every mount point under `base_dir` whose filesystem type matches `fuse`, `fuse.*`, `nfs`, `cifs`, `smb*`, `smbfs`, `afs`, `ceph` or `davfs`, and prunes it from the walk. Use `--verbose` to see which mounts were skipped.

Set `scan_network_mounts: true` only if you really do host working copies on a reliable network share. The scan will then descend into those mounts and print a warning that this can cause long hangs on flaky links.

### Multiple hosts per provider

Each provider key under `hosts` accepts an **array** of hostnames, so you can sync several GitLab or Forgejo instances side-by-side (typical setup: a SaaS one + your self-hosted one).

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

`repos-manager gitlab sync` and `repos-manager sync --all` will then iterate over every configured host. Repositories are mirrored under `<base_dir>/<host>/...`, so each instance gets its own directory tree.

The legacy string form (`"gitlab": "gitlab.com"`) is still accepted for backwards compatibility, and is treated as a single-element list.

**Per-CLI requirements for self-hosted instances:**

- **GitLab**: `repos-manager` sets `GITLAB_HOST` so `glab` targets the right instance. You must have run `glab auth login --hostname <host>` once for each host.
- **Forgejo / Gitea**: `repos-manager` reads the matching login entry from `~/.config/tea/config.yml` (URL + token) and talks to the Forgejo REST API directly. Run `tea login add` once per instance. If no matching login is found, the host is **skipped with a warning** and the sync continues with the next one.
- **GitHub / Bitbucket**: multi-host is not currently wired (single SaaS instance).

You can still override the configured list for a one-off sync with `--host <hostname>`.

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
| `REPOS_MANAGER_BASE_DIR` | Override base directory | `~/Documents` |
| `REPOS_MANAGER_PARALLEL` | Override parallel jobs | `4` |
| `REPOS_MANAGER_PROTOCOL` | Override protocol (`ssh` or `https`) | `ssh` |
| `REPOS_MANAGER_LIB` | Path to lib modules | Auto-detected |
| `REPOS_MANAGER_CONFIG` | Path to config file | `~/.config/repos-manager/config.json` |
| `REPOS_MANAGER_NO_UPDATE_CHECK` | Set to `1` to skip the background update check for one invocation | Unset |
| `REPOS_MANAGER_UPDATE_TTL` | Seconds between update checks (cache TTL) | `86400` |
| `REPOS_MANAGER_UPDATE_CACHE` | Path to the cached latest-version file | `${XDG_CACHE_HOME:-~/.cache}/repos-manager/latest-version` |
| `TEA_CONFIG` | Path to tea's config file, read by the Forgejo provider | `~/.config/tea/config.yml` |
| `NO_COLOR` | Disable colored output | Unset |

Environment variables take precedence over the config file. Flags take precedence over both.
