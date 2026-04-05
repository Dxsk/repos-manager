---
title: Usage
description: Complete command reference for repos-manager.
order: 4
---

## Commands

### login

Authenticate with one or all providers:

```bash
repos-manager login              # All detected providers
repos-manager github login       # GitHub only
repos-manager gitlab login       # GitLab only
```

### sync

Sync repositories from a provider:

```bash
repos-manager github sync                  # Sync GitHub repos
repos-manager sync --all                   # All providers
repos-manager sync --all --parallel 8      # 8 concurrent jobs
repos-manager github sync --filter Dxsk/*  # Only Dxsk's repos
repos-manager sync --all --prune           # Remove deleted remote repos
repos-manager sync --all --dry-run         # Preview without changes
```

### status

Show dirty, ahead, behind, and diverged repos:

```bash
repos-manager status
```

Output:

```bash
github.com/Dxsk/dotenv dirty
github.com/Dxsk/mtd ahead (+2)
gitlab.com/work/api behind (-3)

Total: 42 repos - 39 clean, 1 dirty, 1 ahead, 1 behind, 0 diverged
```

### init

Create a default config file:

```bash
repos-manager init
```

### update

Check for updates and self-update:

```bash
repos-manager update
```

## Flags

| Flag | Description | Default |
|------|-------------|---------|
| `--filter <pattern>` | Filter repos by pattern | None |
| `--base-dir <path>` | Base directory | `~/Documents` |
| `--https` | Use HTTPS instead of SSH | SSH |
| `--prune` | Remove local repos not on remote | Off |
| `--dry-run` | Preview without making changes | Off |
| `--host <host>` | Custom host for self-hosted | Provider default |
| `--parallel <n>` | Concurrent sync jobs | 4 |
| `--verbose`, `-v` | Show debug output | Off |
| `--quiet`, `-q` | Suppress info/success messages (errors still shown) | Off |
