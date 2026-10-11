---
title: Usage
description: "Complete repos-manager command reference: sync, status, login, update, completions, every flag and real examples."
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

Providers are `github`, `gitlab`, `forgejo` (aliases `gitea`, `gogs`), `bitbucket`, `radicle`, `sourcehut` (alias `srht`), `onedev` and `any` (alias `git`).

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
repos-manager sync --all --ignore 'Dxsk/old-*'  # Everything but these
repos-manager sync --all --prune             # Remove deleted remote repos
repos-manager sync --all --dry-run           # Preview without changes
repos-manager sync --all --plan              # Recon summary only, clone nothing
repos-manager sync --all --write-ignore      # Write .repos.yml to pick from
repos-manager sync --all --yes               # No confirmation, whatever the size
```

For each repository:

- missing locally: it is cloned;
- present and clean: `git fetch --all --prune`, then a fast-forward of the remote default branch if it is checked out. Another branch or a detached HEAD is only fetched, so a sync never moves the branch you work on;
- present with uncommitted changes: it is skipped and reported as `dirty`.

`sync --all` skips providers it cannot use, that is those with neither their CLI installed nor saved credentials, and an error on one host never stops the others. `--host` targets a single provider and is rejected with `sync --all`: use `repos-manager <provider> sync --host <host>` instead.

`--prune` removes local repos under the host directory that are no longer listed by the remote. It never deletes:

- a repo with uncommitted or untracked changes, unpushed commits or a stash;
- a repo an `ignore` list catches, or one no `only` list allows (see [Filtering](https://repos-manager.dxscloud.fr/docs/filtering/));
- anything outside the base directory.

When more than half of a host's local repos would go, the listing is treated as incomplete and prune stops: check with `--dry-run`, then add `--force`.

#### Before it clones

Every host is listed first, and a sync says what it is about to do before downloading anything:

```text
github.com   42 in scope, 39 cloned, 3 new (1.2 GB)
             biggest new: org/a 586 MB, org/b 586 MB
gitlab.corp  18 in scope, 0 cloned, 18 new (4.7 GB, 2 unknown)
             biggest new: team/monorepo 3.1 GB, team/assets 900 MB
             with --prune: 2 local repos would be deleted

21 clones, 39 updates, about 5.9 GB to download, 2 of unknown size.
Proceed? [Y/n]
```

`in scope` is what the filters kept for that host, `cloned` what is already on disk, `new` what would be cloned. The volume in brackets is that of the new repositories only, since an update transfers very little.

The figures come from the forge APIs, which report the size of the repository **on the server**, not the size of the transfer: that is why the total says "about". A size a provider does not report is counted as unknown and never estimated, so a host whose new repositories all have unknown sizes shows `?` instead of a figure.

The summary and the question only appear when the run is worth a word: by default 5 new clones or more, or 1 GB or more to download, counted across the whole run rather than per host. A daily sync where nothing is new prints exactly what it has always printed.

The question is asked once per run, `sync --all` included, and only when stdin and stdout are both terminals. A cron job, a CI step or a pipe gets the summary and carries straight on, so nothing can ever wait for an answer that will not come. `--yes` and `--dry-run` print the summary without asking. Answering `n` exits with a success code, clones nothing, and prints how to narrow the scope (`--filter`, `--ignore`, `<base_dir>/.repos.yml`).

A host whose listing came back short already has `--prune` disabled for it, and the summary says so on that host's line, before the question rather than after the run.

See the Configuration page, Sync confirmation, for `confirm_sync` and its thresholds.

#### Planning a big sync

Two flags make that summary a workflow of its own. Neither clones, updates or prunes anything, and both exit with a success code.

```bash
# 1. Reconnaissance: print the summary, then stop
repos-manager sync --all --plan

# 2. Write a file holding every repo in scope, all commented out
repos-manager sync --all --write-ignore

# 3. Uncomment what you do not want in <base_dir>/.repos.yml, then
repos-manager sync --all
```

`--plan` always prints: under the thresholds, under `-q`, and when there is nothing to do, since a summary of zeros answers the question you asked. It never prompts. It is not `--dry-run`: `--dry-run` walks the list and says "would clone / would update" for every repository, `--plan` prints the aggregate summary and nothing else.

`--write-ignore` writes `<base_dir>/.repos.yml` and stops, printing the path it wrote. Every repository is commented out, so the file as generated changes nothing about what gets synced: uncommenting a line is how you drop a repository. It refuses to overwrite an existing `.repos.yml` without `--force`, and `--force` carries the rules already in the file over into the new one, so a rewrite can only ever narrow the scope. See [Filtering](https://repos-manager.dxscloud.fr/docs/filtering/) for the generated file in detail.

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
| `--filter <pattern>` | Only sync matching repos, repeatable | None |
| `--ignore <pattern>` | Skip matching repos, repeatable | None |
| `--base-dir <path>` | Base directory | `~/Documents` |
| `--https` | Use HTTPS instead of SSH | SSH |
| `--prune` | Remove local repos not on remote | Off |
| `--force` | With `--prune`, allow removing most local repos | Off |
| `--dry-run` | Preview without making changes | Off |
| `--yes`, `-y` | Do not ask before cloning | Off |
| `--host <host>` | Custom host for self-hosted (provider sync only) | Configured hosts |
| `--parallel <n>` | Concurrent sync jobs | 4 |
| `--verbose`, `-v` | Show debug output | Off |
| `--quiet`, `-q` | Suppress info/success messages (errors still shown) | Off |

`status` accepts `--base-dir`, `--verbose` and `--quiet`. `update` accepts `--yes` / `-y` too. Both `--flag value` and `--flag=value` forms work.
