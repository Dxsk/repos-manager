#!/usr/bin/env bash
# Forgejo / Gitea provider (uses tea CLI)

forgejo_login() {
    tea login add
}

forgejo_list_repos() {
    if ! command -v tea &>/dev/null; then
        echo "tea CLI not found" >&2; return 1
    fi

    local page=1
    local all_repos="[]"

    while :; do
        local batch
        batch=$(tea repo list --output json --limit 50 --page "$page" 2>/dev/null) || break

        # Empty array or empty output means we're done
        if [[ -z "$batch" ]] || [[ "$(echo "$batch" | jq 'length')" -eq 0 ]]; then
            break
        fi

        all_repos=$(printf '%s\n%s' "$all_repos" "$batch" | jq -s 'add | unique_by(.full_name)')
        page=$((page + 1))
    done

    echo "$all_repos" | jq '[.[] | {
        nameWithOwner: .full_name,
        sshUrl: .ssh_url,
        url: .html_url
    }]'
}

forgejo_get_clone_url() {
    local repo_json="$1"
    if $USE_HTTPS; then
        echo "$repo_json" | jq -r '.url + ".git"'
    else
        echo "$repo_json" | jq -r '.sshUrl'
    fi
}

forgejo_get_full_name() {
    echo "$1" | jq -r '.nameWithOwner'
}
