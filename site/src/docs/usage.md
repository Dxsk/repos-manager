---
title: Usage
description: Complete command reference for repos-manager.
order: 4
---

## Commands

| Command | Description |
|---------|-------------|
| `<provider> login` | Authenticate with one provider |
| `<provider> sync` | Sync repositories from one provider |
| `login [provider]` | Authenticate (all detected providers if none specified) |
| `sync --all` | Sync every provider whose CLI or credentials are available |
| `status` | Show dirty, ahead, behind and diverged repos |
| `init` | Create the default config file |
| `update` | Check for a new release and self-update |
| `completions <shell>` | Print a shell completion script |
| `version` | Print the version |

Providers are `github`, `gitlab`, `forgejo` (alias `gitea`), `bitbucket` and `radicle`.

### login

Authenticate with one or all providers:

```bash
repos-manager login              # All detected providers
repos-manager login gitlab       # GitLab only
repos-manager github login       # GitHub only
```

### sync

Sync repositories from a provider:

```bash
repos-manager github sync                    # Sync GitHub repos
repos-manager sync --all                     # All providers
repos-manager sync --all --parallel 8        # 8 concurrent jobs
repos-manager github sync --filter 'Dxsk/*'  # Only Dxsk's repos
repos-manager sync --all --prune             # Remove deleted remote repos
repos-manager sync --all --dry-run           # Preview without changes
```

For each repository:

- missing locally: it is cloned;
- present and clean: `git fetch --all` then `git pull --ff-only origin HEAD` (works even without upstream tracking);
- present with uncommitted changes: it is skipped and reported as `dirty`.

`sync --all` skips providers whose CLI is not installed, and an error on one host never stops the others. `--host` targets a single provider and is rejected with `sync --all`: use `repos-manager <provider> sync --host <host>` instead.

`--prune` removes local repos under the host directory that are no longer listed by the remote. It is skipped when `--filter` is set and refuses to touch anything outside the base directory. Combine it with `--dry-run` to see what would go.

### status

Show dirty, ahead, behind, and diverged repos:

```bash
repos-manager status
```

Output:

```bash
  github.com/Dxsk/git-identity-manager dirty
  github.com/Dxsk/mtd ahead (+2)
  gitlab.com/work/api behind (-3)

Total: 42 repos - 39 clean, 1 dirty, 1 ahead, 1 behind, 0 diverged
```

While the scan is running, a single-line progress indicator shows the current repo being inspected on stderr (`[N] host/owner/repo`). It is suppressed when stderr is not a TTY or when `--quiet` is set.

`status` walks every `.git` directory under `base_dir`, pruning heavy vendored directories (`node_modules`, `.venv`, `venv`, `__pycache__`, `target`, `vendor`, `dist`, `build`, `.next`, `.cache`) and, on Linux, every network or FUSE mount point under `base_dir` (cloud drives, NFS, SMB, sshfs). Use `--verbose` to see which mounts were skipped, and see the Configuration page, Network-mount scanning, to opt back in.

### init

Create a default config file at `~/.config/repos-manager/config.json` (does nothing if it already exists):

```bash
repos-manager init
```

### update

Check for a newer release and replace the binary in place:

```bash
repos-manager update        # asks for confirmation
repos-manager update --yes  # no prompt
```

The release archive for your platform is downloaded and verified against the release `SHA256SUMS` before the binary is swapped.

In addition to this manual command, `login`, `sync` and `status` print a one-line yellow banner when a newer release is available. The banner reads a cache refreshed at most once a day in the background, so it never slows down the command in progress. See the Configuration page, Update banner, for the opt-outs.

### completions

Print a completion script for `bash`, `zsh`, `fish`, `powershell` or `elvish`:

```bash
repos-manager completions zsh > ~/.local/share/zsh/site-functions/_repos-manager
```

See Getting Started for every shell.

### help

Every command has its own help page:

```bash
repos-manager --help               # General help
repos-manager github --help        # GitHub commands
repos-manager github sync --help   # Sync flags
```

## Flags

Sync flags (`<provider> sync` and `sync --all`):

| Flag | Description | Default |
|------|-------------|---------|
| `--filter <pattern>` | Filter repos by pattern | None |
| `--base-dir <path>` | Base directory | `~/Documents` |
| `--https` | Use HTTPS instead of SSH | SSH |
| `--prune` | Remove local repos not on remote | Off |
| `--dry-run` | Preview without making changes | Off |
| `--host <host>` | Custom host for self-hosted (provider sync only) | Configured hosts |
| `--parallel <n>` | Concurrent sync jobs | 4 |
| `--verbose`, `-v` | Show debug output | Off |
| `--quiet`, `-q` | Suppress info/success messages (errors still shown) | Off |

`status` accepts `--base-dir`, `--verbose` and `--quiet`. `update` accepts `--yes` / `-y`. Both `--flag value` and `--flag=value` forms work.
