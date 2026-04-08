#!/usr/bin/env bats

load test_helper

source "$REPOS_MANAGER_LIB/status.sh"

@test "status: clean repo shows nothing" {
    create_repo "$BASE_DIR/github.com/user/clean-repo"
    run status_all
    echo "DEBUG status=$status" >&3
    echo "DEBUG output=[$output]" >&3
    echo "DEBUG stderr=[$stderr]" >&3
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

@test "status: ignores repos nested under node_modules" {
    # Host the vendored repo as a sibling of the real one so the parent
    # repo does not become dirty from the node_modules/ directory itself.
    mkdir -p "$BASE_DIR/workspace/node_modules/some-dep"
    create_repo "$BASE_DIR/workspace/node_modules/some-dep"
    create_repo "$BASE_DIR/github.com/user/real-repo"
    run status_all
    [[ "$status" -eq 0 ]]
    [[ "$output" =~ "Total: 1 repos" ]]
    [[ ! "$output" =~ "some-dep" ]]
}

@test "status: ignores repos nested under .venv" {
    mkdir -p "$BASE_DIR/workspace/.venv/lib/pkg"
    create_repo "$BASE_DIR/workspace/.venv/lib/pkg"
    create_repo "$BASE_DIR/github.com/user/py-repo"
    run status_all
    [[ "$status" -eq 0 ]]
    [[ "$output" =~ "Total: 1 repos" ]]
}

@test "status: _status_network_mount_points filters by fstype and base" {
    local mountinfo="$TEST_TEMP/mountinfo"
    # Realistic mountinfo lines: a davfs cloud drive under BASE_DIR, an
    # unrelated NFS mount outside BASE_DIR, and a local ext4 mount that
    # must NOT be pruned.
    cat > "$mountinfo" <<EOF
29 1 0:27 / $BASE_DIR/cloud rw,noexec,relatime shared:1 - fuse https://example.com rw
30 1 0:28 / /mnt/nas rw,relatime shared:2 - nfs4 server:/export rw
31 1 0:29 / $BASE_DIR/local rw,relatime shared:3 - ext4 /dev/sda1 rw
32 1 0:30 / $BASE_DIR/sshfs rw,relatime shared:4 - fuse.sshfs user@host:/ rw
33 1 0:31 / $BASE_DIR/dav rw,relatime shared:5 - davfs https://dav.example rw
EOF
    run _status_network_mount_points "$mountinfo"
    [[ "$status" -eq 0 ]]
    [[ "$output" == *"$BASE_DIR/cloud"* ]]
    [[ "$output" == *"$BASE_DIR/sshfs"* ]]
    [[ "$output" == *"$BASE_DIR/dav"* ]]
    [[ "$output" != *"$BASE_DIR/local"* ]]
    [[ "$output" != *"/mnt/nas"* ]]
}

@test "status: _status_network_mount_points returns nothing for empty mountinfo" {
    local mountinfo="$TEST_TEMP/empty_mountinfo"
    : > "$mountinfo"
    run _status_network_mount_points "$mountinfo"
    [[ "$status" -eq 0 ]]
    [[ -z "$output" ]]
}

@test "status: empty base dir" {
    run status_all
    [[ "$status" -eq 0 ]]
    [[ "$output" =~ "0 clean" ]]
}
