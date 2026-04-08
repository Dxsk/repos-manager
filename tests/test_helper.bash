#!/usr/bin/env bash
# Common test helpers

# Force color off so string assertions do not have to deal with ANSI
# escapes. log.sh decides once at source time whether to emit colors
# based on a TTY check, and on some CI runners (notably macOS bats-core
# via brew) the bats process does attach a pseudo-TTY, which would make
# the output contain codes around every flag name.
export NO_COLOR=1

export REPOS_MANAGER_LIB="$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)"

# Source all libs without running main
source "$REPOS_MANAGER_LIB/log.sh"
source "$REPOS_MANAGER_LIB/flags.sh"
source "$REPOS_MANAGER_LIB/config.sh"
source "$REPOS_MANAGER_LIB/match.sh"

# Create a temp directory for each test
setup() {
    TEST_TEMP="$(mktemp -d)"
    export BASE_DIR="$TEST_TEMP/base"
    mkdir -p "$BASE_DIR"
    export GIT_AUTHOR_NAME="Test"
    export GIT_AUTHOR_EMAIL="test@test.com"
    export GIT_COMMITTER_NAME="Test"
    export GIT_COMMITTER_EMAIL="test@test.com"
}

# Clean up after each test
teardown() {
    rm -rf "$TEST_TEMP"
}

# Create a fake git repo
create_repo() {
    local path="$1"
    mkdir -p "$path"
    git -C "$path" init --quiet
    git -C "$path" commit --allow-empty -m "init" --quiet
}

# Create a fake git repo with a remote
create_repo_with_remote() {
    local path="$1"
    local remote_url="${2:-https://github.com/test/repo.git}"
    create_repo "$path"
    git -C "$path" remote add origin "$remote_url"
}

# Create a dirty repo (uncommitted changes)
create_dirty_repo() {
    local path="$1"
    create_repo "$path"
    echo "dirty" > "$path/dirty.txt"
    git -C "$path" add dirty.txt
}

# Create a repo that is ahead of remote
create_ahead_repo() {
    local path="$1"
    local bare="$TEST_TEMP/bare_$(basename "$path").git"

    git init --bare --quiet "$bare"
    git clone --quiet "$bare" "$path" 2>/dev/null
    git -C "$path" commit --allow-empty -m "init" --quiet
    git -C "$path" push --quiet origin main 2>/dev/null
    git -C "$path" commit --allow-empty -m "local commit" --quiet
}
