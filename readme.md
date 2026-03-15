# repos-manager

A single CLI tool to clone and sync all your Git repositories, no matter the provider.

This project is a **template**. Each workspace is an independent instance of repos-manager tied to a specific directory. You can create as many workspaces as you need: one for personal projects, one for work, one per client, one per team -- whatever fits your workflow.

```bash
# Personal repos
mkdir ~/personal && cd ~/personal
nix flake init -t github:Dxsk/repos-manager

# Work repos
mkdir ~/work && cd ~/work
nix flake init -t github:Dxsk/repos-manager

# Client project
mkdir ~/clients/acme && cd ~/clients/acme
nix flake init -t github:Dxsk/repos-manager
```

Each workspace has its own `.repos-filter`, `.repos-ignore`, and `flake.nix`. Configure them independently to sync exactly what you need, where you need it.

## Supported providers

| Provider | CLI | Status |
|----------|-----|--------|
| GitHub | `gh` | Done |
| GitLab | `glab` | Done |
| Forgejo / Gitea | `tea` | Planned |

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
- Whitelist repos via `.repos-filter`
- Shell completions for bash, zsh and fish
- `NO_COLOR` support

## Directory structure

After syncing, your workspace looks like this:

```
my-workspace/
  .repos-filter
  .repos-ignore
  flake.nix
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

<details>
<summary>Nix (recommended)</summary>

Copy a ready-to-use workspace into any directory:

```bash
mkdir ~/my-repos && cd ~/my-repos
nix flake init -t github:Dxsk/repos-manager
```

Enter the dev shell and start syncing:

```bash
nix develop
repos-manager github login
repos-manager github sync
```

The dev shell provides `repos-manager` with all its dependencies and sets `BASE_DIR` to the current directory automatically.

</details>

<details>
<summary>Nix (one-off)</summary>

Run it directly without setting up a workspace:

```bash
nix run github:Dxsk/repos-manager -- github sync
```

</details>

<details>
<summary>Manual</summary>

```bash
git clone git@github.com:Dxsk/repos-manager.git
cd repos-manager
```

Source the file matching your shell:

```bash
# bash
source sourceme.bash

# zsh
source sourceme.zsh

# fish
source sourceme.fish
```

You can add the `source` line to your shell config (`.bashrc`, `.zshrc`, `config.fish`) to make it persistent.

</details>

## Usage

### Authentication

Each provider uses its own CLI for authentication:

```bash
repos-manager github login
repos-manager gitlab login
```

### Syncing repos

```bash
# Sync all repos from GitHub
repos-manager github sync

# Sync all repos from GitLab
repos-manager gitlab sync

# Sync all configured providers at once
repos-manager sync --all
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

# Custom base directory
repos-manager sync --all --base-dir /path/to/repos
```

## Flags

| Flag | Description |
|------|-------------|
| `--filter <pattern>` | Filter repos by pattern (e.g. `Dxsk/*` or `Dxsk/project`) |
| `--base-dir <path>` | Base directory for repos (default: current directory with Nix, `~/Documents` otherwise) |
| `--https` | Use HTTPS clone URLs instead of SSH |
| `--prune` | Remove local repos not found on the remote |
| `--dry-run` | Show what would be done without making any changes |
| `--host <host>` | Custom host for self-hosted instances |

## Filter and ignore files

<details>
<summary>.repos-filter -- whitelist</summary>

Edit `.repos-filter` in your workspace to sync **only** repos that match at least one pattern. If the file contains only comments, all repos are synced.

```
# Only sync repos from Dxsk
Dxsk/*

# Plus a specific repo from another org
other-org/some-project
```

</details>

<details>
<summary>.repos-ignore -- blacklist</summary>

Edit `.repos-ignore` in your workspace to exclude repos from syncing. Applied **after** `.repos-filter`.

```
# Ignore a specific repo
Dxsk/old-project

# Ignore all repos from an owner
test-org/*

# Glob pattern
*/tmp-*
```

</details>

<details>
<summary>Pattern syntax</summary>

Both files use the same syntax:

- Glob wildcards: `*`, `?`
- `owner/*` also matches nested paths (e.g. `group/subgroup/project`)
- Lines starting with `#` are comments
- Empty lines are ignored

</details>

## Environment variables

| Variable | Description | Default |
|----------|-------------|---------|
| `REPOS_MANAGER_BASE_DIR` | Base directory for all repos | Current directory (Nix) or `~/Documents` |
| `REPOS_MANAGER_LIB` | Path to lib modules | Auto-detected |
| `NO_COLOR` | Disable colored output when set | Unset |

## Requirements

When not using Nix, you need these installed:

- `git`
- `jq`
- `gh` for GitHub
- `glab` for GitLab

With Nix, all dependencies are provided automatically.

## License

[MIT](LICENSE)
