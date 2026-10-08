---
title: Filtering
description: Control which repos are synced with filter and ignore patterns.
order: 5
---

## .repos-filter

Create `<base_dir>/.repos-filter` to sync **only** repos matching at least one pattern. If the file is missing, all repos are synced. If it exists but holds no pattern, nothing is synced, so delete it rather than emptying it.

```bash
# Only sync repos from Dxsk
Dxsk/*

# Plus a specific repo from another org
other-org/some-project
```

## .repos-ignore

Create `<base_dir>/.repos-ignore` to exclude repos from syncing. Applied **after** `.repos-filter`. Ignored repos are reported as skipped.

```bash
# Ignore a specific repo
Dxsk/old-project

# Ignore all repos from an owner
test-org/*

# Glob pattern
*/tmp-*
```

## Pattern syntax

- `owner/repo`: exact match (case-sensitive)
- `owner/*`: all repos from an owner (including nested paths like `group/subgroup/repo`)
- `*` and `?` glob wildcards (`*` also crosses `/`, so `*/tmp-*` matches any owner)
- `#` starts a comment, on its own line or after a pattern
- Empty lines are ignored

## --filter flag

The `--filter` flag works the same way but from the command line:

```bash
# Sync only repos from a specific owner
repos-manager github sync --filter 'Dxsk/*'

# Sync a single repo
repos-manager github sync --filter Dxsk/repos-manager
```

## Precedence

1. `--filter` flag (command line)
2. `.repos-filter` file (must match)
3. `.repos-ignore` file (excluded)

A repo must pass all three checks to be synced. Patterns are matched against the remote full name (`owner/repo`), not the local path.

Quote patterns containing `*` on the command line so your shell does not expand them.
