#!/usr/bin/env bash
# GitHub provider (uses gh CLI)

github_login() {
    gh auth login
}

github_list_repos() {
    if ! command -v gh &>/dev/null; then
        echo "gh CLI not found" >&2; return 1
    fi

    local all_repos
    all_repos=$(gh repo list --json nameWithOwner,sshUrl,url --limit 10000 2>/dev/null || echo "[]")

    # Fetch repos from each organization the user belongs to
    local orgs
    orgs=$(gh api user/orgs --jq '.[].login' 2>/dev/null || true)

    for org in $orgs; do
        [[ -z "$org" ]] && continue
        local org_repos
        org_repos=$(gh repo list "$org" --json nameWithOwner,sshUrl,url --limit 10000 2>/dev/null || echo "[]")
        all_repos=$(printf '%s\n%s' "$all_repos" "$org_repos" | jq -s 'add | unique_by(.nameWithOwner)')
    done

    echo "$all_repos"
}

github_get_clone_url() {
    local repo_json="$1"
    if $USE_HTTPS; then
        echo "$repo_json" | jq -r '.url + ".git"'
    else
        echo "$repo_json" | jq -r '.sshUrl'
    fi
}

github_get_full_name() {
    echo "$1" | jq -r '.nameWithOwner'
}
