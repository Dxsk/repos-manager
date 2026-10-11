---
title: Filtering
description: "Choose exactly which repositories repos-manager syncs with .repos.yml allow-lists and deny-lists, per host, plus the --filter and --ignore flags."
order: 5
---

## .repos.yml

One file at the root of the base directory decides what gets synced: `<base_dir>/.repos.yml`.

```yaml
# Everywhere
only:
  - Dxsk/*
ignore:
  - test-org/*

# Per host, on top of the lists above
hosts:
  gitlab.corp:
    only: [team/*]
    ignore: [team/archive-*]
  github.com:
    ignore: [Dxsk/old-project]
```

Both keys are optional, and so is the whole file: with no `.repos.yml` and no legacy text file, everything you can access is synced.

- `only` is an **allow-list**: a repo passes it as soon as it matches **at least one** pattern.
- `ignore` is a **deny-list**: a repo matching any pattern is skipped, and reported as `ignored`.

Host keys are matched against the host name without regard to case, so `GitLab.Corp` and `gitlab.corp` are the same block.

## How the layers combine

Three layers can hold an allow-list, and they are evaluated in this order:

1. the `--filter` flags, taken together as one allow-list;
2. the `hosts.<host>` block of the host being synced;
3. the global `only` list.

A repo has to pass **every** layer that is present. Deny-lists are not layered, they simply add up: `--ignore`, the global `ignore` and the host's `ignore` all apply at once.

The practical consequence is worth spelling out: **a `hosts:` block can only narrow what the global lists already allow, never widen it.** With

```yaml
only: [Dxsk/*]
hosts:
  github.com:
    only: [other-org/*]
```

`other-org/*` is **not** synced on `github.com`: nothing matches both lists, so that host has nothing in scope. repos-manager says so rather than letting it look like an empty remote:

```
⚠ No repository of github.com is in scope: 'hosts.github.com.only' and 'only' have nothing in common
```

This also means a host block can never pull a repository into `--prune`'s reach. It can only take one out, which is the safe direction.

An empty list (`only: []`, globally or per host) is **not** an allow-list that rejects everything. It is ignored, with a warning naming the file and the host, so a generated or half-edited file cannot silently empty your scope.

## Pattern syntax

- `owner/repo`: exact match (case-sensitive)
- `owner/*`: every repo of an owner, nested paths included (`group/subgroup/repo`)
- `*` and `?` glob wildcards (`*` also crosses `/`, so `*/tmp-*` matches any owner)
- patterns are matched against the remote full name (`owner/repo`), not the local path

## --filter and --ignore

The same patterns work from the command line, and both flags are repeatable:

```bash
# One owner, or a single repo
repos-manager github sync --filter 'Dxsk/*'
repos-manager github sync --filter Dxsk/repos-manager

# Several patterns: a repo matching any of them passes
repos-manager sync --all --filter 'Dxsk/*' --filter 'other-org/tooling'

# Everything but a few
repos-manager sync --all --ignore 'Dxsk/old-*' --ignore 'test-org/*'
```

Quote patterns containing `*` so your shell does not expand them.

## Let repos-manager write it

`--write-ignore` generates `.repos.yml` from what a sync would actually do, so you pick from a real list instead of guessing names:

```bash
repos-manager sync --all --write-ignore
```

It writes the file, prints the path and stops. Nothing is cloned, updated or pruned.

```yaml
# Written by repos-manager on 2026-10-11.
# Uncomment a repository to stop syncing it. Sizes are what the forge reports.

hosts:
  github.com:
    ignore:
      # - 'Dxsk/monorepo'          # 3.1 GB
      # - 'Dxsk/assets'            # 900 MB
      # - 'Dxsk/notes'             # ?
  git.sr.ht:
    ignore:
      # - '~dxsk/dotfiles'         # 12 MB
```

How to read it:

- one block per host, each repository **in scope** on its own commented line, biggest first;
- the trailing comment is the size the forge reports, `?` when it reports none, and those come last;
- sizes are what the server holds, not what a clone transfers, so treat them as an order of magnitude;
- names are quoted because a Sourcehut name starts with `~` and a glob can start with `*`, neither of which is a valid bare YAML scalar.

**Uncomment what you do not want**, then sync. The file as generated changes nothing: every entry is commented, so delivering it untouched syncs exactly what you synced before. The generator only ever emits `ignore` lists, never an `only` list, so no file it writes can empty your scope by accident.

It refuses to overwrite an existing `.repos.yml`:

```
<base_dir>/.repos.yml already exists, add --force to overwrite it
```

With `--force`, the rules already in the file are carried over into the new one, uncommented, above the catalogue. A rewrite can therefore only ever narrow what you sync, never widen it, which matters because dropping an `only:` list would silently pull every repository of a host back into scope, `--prune` included.

Pair it with `--plan` to see the summary first:

```bash
repos-manager sync --all --plan            # how much there is, per host
repos-manager sync --all --write-ignore    # the list to pick from
```

## When something is wrong with the file

`.repos.yml` drives `--prune`, so a file repos-manager cannot trust stops the run instead of falling back to "no filter":

- an unknown key (`onlyy:`, `host:`, a misplaced `only:`) is an error naming the key;
- a malformed YAML document is an error naming the file;
- a file that cannot be read is an error naming the file.

No host is synced and nothing is pruned until the file parses.

## Legacy text files

The two 1.0 text files still work, unchanged, **as long as there is no `.repos.yml`**:

- `<base_dir>/.repos-filter`, one allow-list pattern per line;
- `<base_dir>/.repos-ignore`, one deny-list pattern per line.

Both accept `#` comments and blank lines. A `.repos-filter` that exists but holds no pattern still lets nothing through, which is why the YAML `only: []` behaves differently.

As soon as a `.repos.yml` exists it takes over completely, and repos-manager warns once that the text files are being ignored. Moving over is a copy and paste:

```yaml
# .repos-filter          ->  only:
# .repos-ignore          ->  ignore:
only:
  - Dxsk/*
  - other-org/some-project
ignore:
  - Dxsk/old-project
  - test-org/*
```
