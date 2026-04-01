#!/usr/bin/env bats

load test_helper

source "$REPOS_MANAGER_LIB/sync.sh"

_setup_bare_and_clone() {
    local bare="$TEST_TEMP/remote.git"
    local clone="$1"
    git init --bare --quiet "$bare"
    git clone --quiet "$bare" "$TEST_TEMP/_seed" 2>/dev/null
    git -C "$TEST_TEMP/_seed" commit --allow-empty -m "init" --quiet
    git -C "$TEST_TEMP/_seed" push --quiet 2>/dev/null
    if [[ -n "$clone" ]]; then
        git clone --quiet "$bare" "$clone" 2>/dev/null
    fi
    echo "$bare"
}

@test "sync_repo: clones new repo" {
    local bare
    bare=$(_setup_bare_and_clone "")
    local result
    result=$(sync_repo "github" "$TEST_TEMP/clone" "$bare" "user/repo" 2>/dev/null)
    [[ "$result" == "cloned" ]]
    [[ -d "$TEST_TEMP/clone/.git" ]]
}

@test "sync_repo: updates existing repo" {
    local bare
    bare=$(_setup_bare_and_clone "$TEST_TEMP/repo")
    local result
    result=$(sync_repo "github" "$TEST_TEMP/repo" "$bare" "user/repo" 2>/dev/null)
    [[ "$result" == "updated" ]]
}

@test "sync_repo: skips dirty repo" {
    local bare
    bare=$(_setup_bare_and_clone "$TEST_TEMP/repo")
    echo "dirty" > "$TEST_TEMP/repo/file.txt"
    git -C "$TEST_TEMP/repo" add file.txt
    local result
    result=$(sync_repo "github" "$TEST_TEMP/repo" "$bare" "user/repo" 2>/dev/null)
    [[ "$result" == "skipped" ]]
}

@test "sync_repo: clone fails on bad url" {
    local result
    result=$(sync_repo "github" "$TEST_TEMP/fail" "/nonexistent.git" "user/repo" 2>/dev/null)
    [[ "$result" == "errored" ]]
}
