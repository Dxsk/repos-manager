---
title: Filtering
description: Control which repos are synced with filter and ignore patterns.
order: 5
---

## .repos-filter

Create `$BASE_DIR/.repos-filter` to sync **only** repos matching at least one pattern. If the file is empty or missing, all repos are synced.

```bash
# Only sync repos from Dxsk
Dxsk/*

# Plus a specific repo from another org
other-org/some-project
```

## .repos-ignore

Create `$BASE_DIR/.repos-ignore` to exclude repos from syncing. Applied **after** `.repos-filter`.

```bash
# Ignore a specific repo
Dxsk/old-project

# Ignore all repos from an owner
test-org/*

# Glob pattern
*/tmp-*
```

## Pattern syntax

- `owner/repo` - exact match
- `owner/*` - all repos from an owner (including nested paths like `group/subgroup/repo`)
- `*` and `?` glob wildcards
- Lines starting with `#` are comments
- Empty lines are ignored

## --filter flag

The `--filter` flag works the same way but from the command line:

```bash
# Sync only repos from a specific owner
repos-manager github sync --filter Dxsk/*

# Sync a single repo
repos-manager github sync --filter Dxsk/repos-manager
```

## Precedence

1. `--filter` flag (command line)
2. `.repos-filter` file (must match)
3. `.repos-ignore` file (excluded)

A repo must pass all three checks to be synced.
