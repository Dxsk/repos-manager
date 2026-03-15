#!/usr/bin/env bash
# GitLab provider (uses glab CLI)

gitlab_login() {
    glab auth login
}

gitlab_list_repos() {
    if ! command -v glab &>/dev/null; then
        echo "glab CLI not found" >&2; return 1
    fi

    local raw
    raw=$(glab api "projects?membership=true&per_page=100&simple=true" --paginate 2>/dev/null || echo "[]")

    # glab --paginate may return concatenated JSON arrays; slurp merges them
    echo "$raw" | jq -s 'add // []' | jq '[.[] | {
        nameWithOwner: .path_with_namespace,
        sshUrl: .ssh_url_to_repo,
        url: .http_url_to_repo
    }]'
}

gitlab_get_clone_url() {
    local repo_json="$1"
    if $USE_HTTPS; then
        echo "$repo_json" | jq -r '.url'
    else
        echo "$repo_json" | jq -r '.sshUrl'
    fi
}

gitlab_get_full_name() {
    echo "$1" | jq -r '.nameWithOwner'
}
