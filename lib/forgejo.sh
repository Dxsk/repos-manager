#!/usr/bin/env bash
# Forgejo / Gitea provider (uses tea CLI)

forgejo_login() {
    tea login add
}

_forgejo_login_for_host() {
    local host="$1"
    [[ -z "$host" ]] && return 0
    # tea logins list outputs columns: NAME URL SSHHOST USER ...
    # Match a login whose URL host equals the requested host.
    tea logins list 2>/dev/null | awk -v h="$host" '
        NR == 1 { next }  # header
        {
            url = $2
            sub("^https?://", "", url)
            sub("/.*$", "", url)
            if (url == h) { print $1; exit }
        }
    '
}

forgejo_list_repos() {
    if ! command -v tea &>/dev/null; then
        echo "tea CLI not found" >&2; return 1
    fi

    local -a tea_args=()
    if [[ -n "${CURRENT_HOST:-}" ]]; then
        local login
        login=$(_forgejo_login_for_host "$CURRENT_HOST")
        if [[ -n "$login" ]]; then
            tea_args=(--login "$login")
        else
            echo "No tea login matches host '${CURRENT_HOST}'. Run: tea login add" >&2
            return 1
        fi
    fi

    local page=1
    local all_repos="[]"

    while :; do
        local batch
        batch=$(tea repo list "${tea_args[@]}" --output json --limit 50 --page "$page" 2>/dev/null) || break

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
