#!/usr/bin/env bash
# GitHub provider (uses gh CLI)

github_login() {
    gh auth login
}

github_list_repos() {
    if ! command -v gh &>/dev/null; then
        echo "gh CLI not found" >&2; return 1
    fi

    # `gh repo list` only returns the authenticated user's own repos; repos
    # where the user is a plain collaborator on someone else's personal
    # account would never appear. Hit /user/repos directly with the full
    # affiliation set so owner, collaborator and organization_member repos
    # are all listed in a single paginated call.
    gh api --paginate \
        '/user/repos?affiliation=owner,collaborator,organization_member&per_page=100' \
        --jq '[.[] | {nameWithOwner: .full_name, sshUrl: .ssh_url, url: .html_url}]' \
        2>/dev/null \
        | jq -s 'add // [] | unique_by(.nameWithOwner)'
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
