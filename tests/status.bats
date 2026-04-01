#!/usr/bin/env bats

load test_helper

source "$REPOS_MANAGER_LIB/status.sh"

@test "status: clean repo shows nothing" {
    create_repo "$BASE_DIR/github.com/user/clean-repo"
    run status_all
    [[ "$status" -eq 0 ]]
    [[ "$output" =~ "1 clean" ]]
}

@test "status: dirty repo detected" {
    create_dirty_repo "$BASE_DIR/github.com/user/dirty-repo"
    run status_all
    [[ "$status" -eq 0 ]]
    [[ "$output" =~ "dirty" ]]
    [[ "$output" =~ "1 dirty" ]]
}

@test "status: ahead repo detected" {
    create_ahead_repo "$BASE_DIR/github.com/user/ahead-repo"
    run status_all
    [[ "$status" -eq 0 ]]
    [[ "$output" =~ "ahead" ]]
}

@test "status: multiple repos counted" {
    create_repo "$BASE_DIR/github.com/user/repo1"
    create_repo "$BASE_DIR/github.com/user/repo2"
    create_dirty_repo "$BASE_DIR/github.com/user/repo3"
    run status_all
    [[ "$status" -eq 0 ]]
    [[ "$output" =~ "2 clean" ]]
    [[ "$output" =~ "1 dirty" ]]
}

@test "status: empty base dir" {
    run status_all
    [[ "$status" -eq 0 ]]
    [[ "$output" =~ "0 clean" ]]
}
