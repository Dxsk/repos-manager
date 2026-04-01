#!/usr/bin/env bats

load test_helper

# ── match_pattern ──────────────────────────────────────────────────────────────

@test "match_pattern: exact match" {
    match_pattern "Dxsk/repo" "Dxsk/repo"
}

@test "match_pattern: exact match fails on different name" {
    run match_pattern "Dxsk/repo" "Dxsk/other"
    [[ "$status" -ne 0 ]]
}

@test "match_pattern: wildcard owner/*" {
    match_pattern "Dxsk/*" "Dxsk/any-repo"
}

@test "match_pattern: wildcard matches nested" {
    match_pattern "Dxsk/*" "Dxsk/sub/repo"
}

@test "match_pattern: wildcard fails on different owner" {
    run match_pattern "Dxsk/*" "Other/repo"
    [[ "$status" -ne 0 ]]
}

@test "match_pattern: glob ? wildcard" {
    match_pattern "Dxsk/repo-?" "Dxsk/repo-1"
}

# ── load_patterns ──────────────────────────────────────────────────────────────

@test "load_patterns: reads lines, strips comments and whitespace" {
    cat > "$TEST_TEMP/patterns" <<'EOF'
  Dxsk/*
# This is a comment
  Other/repo

EOF
    local result
    result=$(load_patterns "$TEST_TEMP/patterns")
    [[ "$(echo "$result" | wc -l)" -eq 2 ]]
    echo "$result" | grep -q "Dxsk/\*"
    echo "$result" | grep -q "Other/repo"
}

@test "load_patterns: nonexistent file returns nothing" {
    local result
    result=$(load_patterns "$TEST_TEMP/nonexistent")
    [[ -z "$result" ]]
}

# ── is_ignored ──────────────────────────────────────────────────────────────────

@test "is_ignored: matches pattern in .repos-ignore" {
    echo "Dxsk/old-project" > "$BASE_DIR/.repos-ignore"
    is_ignored "Dxsk/old-project"
}

@test "is_ignored: does not match unlisted repo" {
    echo "Dxsk/old-project" > "$BASE_DIR/.repos-ignore"
    run is_ignored "Dxsk/new-project"
    [[ "$status" -ne 0 ]]
}

@test "is_ignored: wildcard ignore" {
    echo "test-org/*" > "$BASE_DIR/.repos-ignore"
    is_ignored "test-org/any-repo"
}

@test "is_ignored: no ignore file means nothing ignored" {
    run is_ignored "Dxsk/repo"
    [[ "$status" -ne 0 ]]
}

# ── is_filtered_out ─────────────────────────────────────────────────────────────

@test "is_filtered_out: no filter file allows everything" {
    run is_filtered_out "anything/repo"
    [[ "$status" -ne 0 ]]  # returns 1 = not filtered out = allowed
}

@test "is_filtered_out: matching repo passes filter" {
    echo "Dxsk/*" > "$BASE_DIR/.repos-filter"
    run is_filtered_out "Dxsk/repo"
    [[ "$status" -ne 0 ]]  # returns 1 = allowed
}

@test "is_filtered_out: non-matching repo is filtered out" {
    echo "Dxsk/*" > "$BASE_DIR/.repos-filter"
    is_filtered_out "Other/repo"  # returns 0 = filtered out
}
