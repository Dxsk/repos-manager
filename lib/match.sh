#!/usr/bin/env bash
# Pattern matching and .repos-ignore support

# Match a glob pattern against a repo name.
# Patterns ending with /* also match nested paths:
#   "Dxsk/*"  matches "Dxsk/foo" and "Dxsk/sub/repo"
#   "Dxsk/git-chronicles" matches exactly
match_pattern() {
    local pattern="$1" name="$2"

    # shellcheck disable=SC2053
    [[ "$name" == $pattern ]] && return 0

    # Recursive match for patterns ending with /*
    if [[ "$pattern" == *"/*" ]]; then
        local prefix="${pattern%/*}"
        [[ "$name" == "$prefix"/* ]] && return 0
    fi

    return 1
}

# Read patterns from a file (strips comments and whitespace)
# Returns patterns via stdout, one per line
load_patterns() {
    local file="$1"
    [[ ! -f "$file" ]] && return

    while IFS= read -r line || [[ -n "$line" ]]; do
        line="${line%%#*}"
        line="${line#"${line%%[![:space:]]*}"}"
        line="${line%"${line##*[![:space:]]}"}"
        [[ -z "$line" ]] && continue
        echo "$line"
    done < "$file"
}

# Check if a name matches any pattern from a file
matches_file() {
    local name="$1" file="$2"
    [[ ! -f "$file" ]] && return 1

    local pattern
    while IFS= read -r pattern; do
        match_pattern "$pattern" "$name" && return 0
    done < <(load_patterns "$file")
    return 1
}

# Check if a repo should be ignored (.repos-ignore)
is_ignored() {
    matches_file "$1" "$BASE_DIR/.repos-ignore"
}

# Check if a repo matches the filter file (.repos-filter)
# Returns 0 (true) if repo is allowed, 1 if filtered out
is_filtered_out() {
    local name="$1"
    local filter_file="$BASE_DIR/.repos-filter"
    # No filter file = everything passes
    [[ ! -f "$filter_file" ]] && return 1
    # File exists: repo must match at least one pattern
    matches_file "$name" "$filter_file" && return 1
    return 0
}
