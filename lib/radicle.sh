#!/usr/bin/env bash
# Radicle provider (uses rad CLI)

radicle_login() {
    if ! command -v rad &>/dev/null; then
        log_error "rad CLI not found. Install from https://radicle.xyz"
        return 1
    fi
    rad auth
}

radicle_list_repos() {
    if ! command -v rad &>/dev/null; then
        echo "rad CLI not found" >&2; return 1
    fi

    # List all tracked repos from the radicle node
    local repos
    repos=$(rad ls --json 2>/dev/null || echo "[]")

    echo "$repos" | jq '[.[] | {
        nameWithOwner: (.namespace + "/" + .name),
        sshUrl: ("rad://" + .id),
        url: ("rad://" + .id)
    }]' 2>/dev/null || echo "[]"
}

radicle_get_clone_url() {
    local repo_json="$1"
    echo "$repo_json" | jq -r '.sshUrl'
}

radicle_get_full_name() {
    echo "$1" | jq -r '.nameWithOwner'
}
