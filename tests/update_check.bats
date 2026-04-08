#!/usr/bin/env bats

load test_helper

setup() {
    TEST_TEMP="$(mktemp -d)"
    export REPOS_MANAGER_UPDATE_CACHE="$TEST_TEMP/latest-version"
    export REPOS_MANAGER_UPDATE_TTL=60
    export CHECK_UPDATES="true"
    unset REPOS_MANAGER_NO_UPDATE_CHECK
    # shellcheck source=../lib/update_check.sh
    source "$REPOS_MANAGER_LIB/update_check.sh"
}

teardown() {
    rm -rf "$TEST_TEMP"
}

@test "update_check_banner: silent when cache missing" {
    VERSION="0.5.0"
    run update_check_banner
    [[ "$status" -eq 0 ]]
    [[ -z "$output" ]]
}

@test "update_check_banner: silent when cached equals current" {
    VERSION="0.5.0"
    echo "0.5.0" > "$REPOS_MANAGER_UPDATE_CACHE"
    run update_check_banner
    [[ "$status" -eq 0 ]]
    [[ -z "$output" ]]
}

@test "update_check_banner: announces when cached is newer" {
    VERSION="0.5.0"
    echo "0.5.1" > "$REPOS_MANAGER_UPDATE_CACHE"
    run update_check_banner
    [[ "$status" -eq 0 ]]
    [[ "$output" == *"0.5.1 available"* ]]
    [[ "$output" == *"current 0.5.0"* ]]
}

@test "update_check_banner: silent when cached is older" {
    VERSION="0.5.2"
    echo "0.5.1" > "$REPOS_MANAGER_UPDATE_CACHE"
    run update_check_banner
    [[ "$status" -eq 0 ]]
    [[ -z "$output" ]]
}

@test "update_check_banner: NO_UPDATE_CHECK env disables banner" {
    VERSION="0.5.0"
    echo "0.9.9" > "$REPOS_MANAGER_UPDATE_CACHE"
    REPOS_MANAGER_NO_UPDATE_CHECK=1 run update_check_banner
    [[ "$status" -eq 0 ]]
    [[ -z "$output" ]]
}

@test "update_check_banner: CHECK_UPDATES=false disables banner" {
    VERSION="0.5.0"
    echo "0.9.9" > "$REPOS_MANAGER_UPDATE_CACHE"
    CHECK_UPDATES="false" run update_check_banner
    [[ "$status" -eq 0 ]]
    [[ -z "$output" ]]
}

@test "update_check_cache_stale: true when file missing" {
    run _update_check_cache_stale
    [[ "$status" -eq 0 ]]
}

@test "update_check_cache_stale: false when file fresh" {
    echo "0.5.0" > "$REPOS_MANAGER_UPDATE_CACHE"
    run _update_check_cache_stale
    [[ "$status" -ne 0 ]]
}

@test "update_check_cache_stale: true when file older than TTL" {
    echo "0.5.0" > "$REPOS_MANAGER_UPDATE_CACHE"
    # Backdate the file beyond the TTL
    touch -d "@1" "$REPOS_MANAGER_UPDATE_CACHE" 2>/dev/null \
        || touch -t 197001010000 "$REPOS_MANAGER_UPDATE_CACHE"
    run _update_check_cache_stale
    [[ "$status" -eq 0 ]]
}
