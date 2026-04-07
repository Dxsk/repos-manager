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
| `hosts.<provider>` | List of hosts to sync for that provider | See defaults below |

### Multiple hosts per provider

Each provider key under `hosts` accepts an **array** of hostnames, so you can sync several GitLab or Forgejo instances side-by-side (typical setup: a SaaS one + your self-hosted one).

{% raw %}
```json
{
  "hosts": {
    "gitlab":  ["gitlab.com", "gitlab.babel.coop"],
    "forgejo": ["codeberg.org", "forge.babel.coop"]
  }
}
```
{% endraw %}

`repos-manager gitlab sync` and `repos-manager sync --all` will then iterate over every configured host. Repositories are mirrored under `<base_dir>/<host>/...`, so each instance gets its own directory tree.

The legacy string form (`"gitlab": "gitlab.com"`) is still accepted for backwards compatibility — it is treated as a single-element list.

**Per-CLI requirements for self-hosted instances:**

- **GitLab** — `repos-manager` sets `GITLAB_HOST` so `glab` targets the right instance. You must have run `glab auth login --hostname <host>` once for each host.
- **Forgejo / Gitea** — `repos-manager` looks up a `tea` login whose URL host matches the configured hostname and passes `--login <name>`. Run `tea login add` once per instance.
- **GitHub / Bitbucket** — multi-host is not currently wired (single SaaS instance).

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
| `NO_COLOR` | Disable colored output | Unset |

Environment variables take precedence over the config file. Flags take precedence over both.
