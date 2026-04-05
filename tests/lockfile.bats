#!/usr/bin/env bats

load test_helper

# Source sync module
source "$REPOS_MANAGER_LIB/sync.sh"

@test "acquire_lock creates lockfile" {
    acquire_lock "$TEST_TEMP"
    [[ -f "$TEST_TEMP/.repos-manager.lock" ]]
    release_lock "$TEST_TEMP"
}

@test "acquire_lock fails if lock already held" {
    sleep 60 &
    local pid=$!
    echo "$pid" > "$TEST_TEMP/.repos-manager.lock"

    run acquire_lock "$TEST_TEMP"
    [[ "$status" -ne 0 ]]

    kill "$pid" 2>/dev/null || true
    rm -f "$TEST_TEMP/.repos-manager.lock"
}

@test "acquire_lock succeeds if lock held by dead process" {
    echo "99999999" > "$TEST_TEMP/.repos-manager.lock"
    acquire_lock "$TEST_TEMP"
    [[ -f "$TEST_TEMP/.repos-manager.lock" ]]
    release_lock "$TEST_TEMP"
}

@test "release_lock removes lockfile" {
    acquire_lock "$TEST_TEMP"
    release_lock "$TEST_TEMP"
    [[ ! -f "$TEST_TEMP/.repos-manager.lock" ]]
}
