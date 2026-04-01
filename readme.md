# repos-manager

A single CLI tool to clone and sync all your Git repositories, no matter the provider.

## Supported providers

| Provider | CLI | Status |
|----------|-----|--------|
| GitHub | `gh` | Done |
| GitLab | `glab` | Done |
| Forgejo / Gitea | `tea` | Done |
| Bitbucket | `bitbucket` or API | Done |
| Radicle | `rad` | Done |

## Features

- Clone all accessible repos (personal, orgs, groups, subgroups)
- Mirror the remote namespace hierarchy locally: `provider/owner/repo`
- Update existing repos with fetch + fast-forward pull
- Skip repos with uncommitted local changes
- Remove local repos that no longer exist on the remote (`--prune`)
- Preview changes before applying them (`--dry-run`)
- SSH and HTTPS support
- Filter by owner or specific repo (`--filter`)
- Exclude repos via `.repos-ignore`
- Filter repos via `.repos-filter`
- Parallel sync (default: 4 jobs, configurable with `--parallel`)
- Status overview: dirty, ahead, behind, diverged repos
- Universal login: authenticate all detected providers at once
- Config file (`~/.config/repos-manager/config.json`) for defaults
- Auto-generated sourceme files per host directory
- Shell completions for bash, zsh and fish
- `NO_COLOR` support

## Directory structure

After syncing, your workspace looks like this:

```
~/Documents/
  .repos-filter
  .repos-ignore
  github.com/
    dxsk/
      my-project/
    my-org/
      other-project/
  gitlab.com/
    my-user/
      project/
    my-group/
      sub-group/
        project/
```

## Installation

### Arch / CachyOS

```bash
git clone git@github.com:Dxsk/repos-manager.git
cd repos-manager
make install
```

This installs to `~/.local/bin/repos-manager`. To change the prefix:

```bash
make install PREFIX=/usr/local
```

### Manual (any distro)

Source the file matching your shell:

```bash
# bash
source sourceme.bash

# zsh
source sourceme.zsh

# fish
source sourceme.fish
```

You can add the `source` line to your shell config to make it persistent.

### Nix

```bash
nix run github:Dxsk/repos-manager -- github sync
```

Or use as a flake template:

```bash
mkdir ~/my-repos && cd ~/my-repos
nix flake init -t github:Dxsk/repos-manager
nix develop
```

## Usage

### Configuration

Generate a default config file:

```bash
repos-manager init
# Creates ~/.config/repos-manager/config.json
```

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

### Authentication

```bash
# Login all detected providers at once
repos-manager login

# Or login a specific provider
repos-manager github login
repos-manager gitlab login
repos-manager forgejo login
repos-manager bitbucket login
repos-manager radicle login
```

### Syncing repos

```bash
# Sync all repos from GitHub
repos-manager github sync

# Sync all repos from GitLab
repos-manager gitlab sync

# Sync all configured providers at once
repos-manager sync --all

# Parallel sync with 8 jobs
repos-manager sync --all --parallel 8
```

### Status

Check which repos have uncommitted changes, are ahead/behind, or diverged:

```bash
repos-manager status
```

### Filtering

```bash
# Sync only repos from a specific owner
repos-manager github sync --filter Dxsk/*

# Sync a single repo
repos-manager github sync --filter Dxsk/repos-manager
```

### Other options

```bash
# Use HTTPS instead of SSH
repos-manager github sync --https

# Remove local repos that no longer exist on the remote
repos-manager sync --all --prune

# Preview what would happen without making changes
repos-manager sync --all --dry-run

# GitLab self-hosted
repos-manager gitlab sync --host gitlab.self-hosted.com

# Forgejo / Gitea
repos-manager forgejo sync --host forgejo.self-hosted.com

# Custom base directory
repos-manager sync --all --base-dir /path/to/repos
```

## Flags

| Flag | Description |
|------|-------------|
| `--filter <pattern>` | Filter repos by pattern (e.g. `Dxsk/*` or `Dxsk/project`) |
| `--base-dir <path>` | Base directory for repos (default: `~/Documents`) |
| `--https` | Use HTTPS clone URLs instead of SSH |
| `--prune` | Remove local repos not found on the remote |
| `--dry-run` | Show what would be done without making any changes |
| `--host <host>` | Custom host for self-hosted instances |
| `--parallel <n>` | Number of parallel sync jobs (default: 4) |

## Auto-generated sourceme files

After syncing, repos-manager generates `sourceme`, `sourceme.zsh`, and `sourceme.fish` files in each host directory (e.g. `~/Documents/github.com/`). These files provide the `repos-manager` function when you `cd` into the directory.

If you use the [sourceme auto-loader](https://github.com/Dxsk/dotenv), the function is loaded/unloaded automatically as you navigate.

## Filter and ignore files

### .repos-filter

Sync **only** repos matching at least one pattern. If the file is empty or missing, all repos are synced.

```
# Only sync repos from Dxsk
Dxsk/*

# Plus a specific repo from another org
other-org/some-project
```

### .repos-ignore

Exclude repos from syncing. Applied **after** `.repos-filter`.

```
# Ignore a specific repo
Dxsk/old-project

# Ignore all repos from an owner
test-org/*
```

### Pattern syntax

- Glob wildcards: `*`, `?`
- `owner/*` also matches nested paths (e.g. `group/subgroup/project`)
- Lines starting with `#` are comments
- Empty lines are ignored

## Environment variables

| Variable | Description | Default |
|----------|-------------|---------|
| `REPOS_MANAGER_BASE_DIR` | Base directory for all repos | `~/Documents` |
| `REPOS_MANAGER_LIB` | Path to lib modules | Auto-detected |
| `NO_COLOR` | Disable colored output when set | Unset |

## Requirements

- `git`
- `jq`
- `gh` for GitHub
- `glab` for GitLab
- `tea` for Forgejo / Gitea
- `bitbucket` for Bitbucket (optional, can use API fallback)
- `rad` for Radicle

## License

[MIT](LICENSE)
