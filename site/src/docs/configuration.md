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
    "gitlab": "gitlab.com",
    "forgejo": "gitea.com"
  }
}
```
{% endraw %}

| Key | Description | Default |
|-----|-------------|---------|
| `base_dir` | Root directory for all repos | `~/Documents` |
| `parallel` | Number of parallel sync jobs | `4` |
| `protocol` | Clone protocol (`ssh` or `https`) | `ssh` |
| `hosts.gitlab` | GitLab host | `gitlab.com` |
| `hosts.forgejo` | Forgejo/Gitea host | `gitea.com` |

## Environment variables

| Variable | Description | Default |
|----------|-------------|---------|
| `REPOS_MANAGER_BASE_DIR` | Override base directory | `~/Documents` |
| `REPOS_MANAGER_LIB` | Path to lib modules | Auto-detected |
| `REPOS_MANAGER_CONFIG` | Path to config file | `~/.config/repos-manager/config.json` |
| `NO_COLOR` | Disable colored output | Unset |

Environment variables take precedence over the config file. Flags take precedence over both.
