#!/usr/bin/env bash
# Forgejo / Gitea provider
#
# Auth is bootstrapped via `tea login add` (OAuth or token), but listing is
# done directly against the Forgejo API by reading the token from tea's
# config file. `tea repo list` only returns the authenticated user's own
# repos and does not enumerate organizations, so hitting the API is the
# only robust way to get every accessible repo in a single place.

TEA_CONFIG="${TEA_CONFIG:-$HOME/.config/tea/config.yml}"

forgejo_login() {
    if ! command -v tea &>/dev/null; then
        echo "tea CLI not found. Install it: https://gitea.com/gitea/tea" >&2
        return 1
    fi
    tea login add
}

# Ensure deps required for API-based listing are present.
# jq is already checked globally; we additionally need yq and curl here.
_forgejo_check_deps() {
    local missing=()
    command -v curl &>/dev/null || missing+=("curl")
    command -v yq   &>/dev/null || missing+=("yq")
    if [[ ${#missing[@]} -gt 0 ]]; then
        echo "forgejo provider requires: ${missing[*]}" >&2
        echo "Install them and retry (tea is only used for 'login add')." >&2
        return 1
    fi
}

# Print "url<TAB>token" for the tea login whose host matches $1.
# Returns 2 if no matching login is found (signals "skip this host").
#
# We keep the yq filter intentionally trivial (no --arg, no sub()) so it
# behaves identically under mikefarah/yq (Go) and python-yq (jq wrapper),
# which disagree on `--arg` and a few string primitives. All host matching
# happens in bash against the plain "url<TAB>token" stream.
_forgejo_creds_for_host() {
    local host="$1"
    [[ -z "$host" ]] && return 1
    [[ -f "$TEA_CONFIG" ]] || { echo "tea config not found: $TEA_CONFIG" >&2; return 2; }

    local url token stripped
    while IFS=$'\t' read -r url token; do
        [[ -z "$url" || "$url" == "null" ]] && continue
        [[ -z "$token" || "$token" == "null" ]] && continue
        # Strip scheme and anything after the host.
        stripped="${url#http://}"
        stripped="${stripped#https://}"
        stripped="${stripped%%/*}"
        if [[ "$stripped" == "$host" ]]; then
            printf '%s\t%s\n' "${url%/}" "$token"
            return 0
        fi
    done < <(yq -r '.logins[] | [.url, .token] | @tsv' "$TEA_CONFIG" 2>/dev/null)

    return 2
}

# Paginate a Forgejo API endpoint and echo a JSON array of all results.
# Args: base_url token path  (path starts with /api/v1/...)
_forgejo_api_paginate() {
    local base="$1" token="$2" path="$3"
    local page=1 all="[]" sep='?'
    [[ "$path" == *'?'* ]] && sep='&'

    while :; do
        local body http
        body=$(curl -sS -w $'\n%{http_code}' \
            -H "Authorization: token ${token}" \
            -H "Accept: application/json" \
            "${base}${path}${sep}limit=50&page=${page}") || return 1
        http="${body##*$'\n'}"
        body="${body%$'\n'*}"

        if [[ "$http" != "200" ]]; then
            echo "Forgejo API ${path} returned HTTP ${http}" >&2
            return 1
        fi

        # Non-array response (e.g. error object) → stop.
        if ! echo "$body" | jq -e 'type == "array"' &>/dev/null; then
            break
        fi

        local n
        n=$(echo "$body" | jq 'length')
        [[ "$n" -eq 0 ]] && break

        all=$(printf '%s\n%s' "$all" "$body" | jq -s 'add')
        [[ "$n" -lt 50 ]] && break
        page=$((page + 1))
    done

    echo "$all"
}

forgejo_list_repos() {
    _forgejo_check_deps || return 1

    if [[ -z "${CURRENT_HOST:-}" ]]; then
        echo "forgejo: CURRENT_HOST is not set" >&2
        return 1
    fi

    local creds url token rc=0
    creds=$(_forgejo_creds_for_host "$CURRENT_HOST") || rc=$?
    if [[ $rc -ne 0 ]]; then
        echo "No tea login matches host '${CURRENT_HOST}'. Run: tea login add" >&2
        return 2
    fi
    url="${creds%%$'\t'*}"
    token="${creds##*$'\t'}"

    # 1. User's own repos
    local user_repos
    user_repos=$(_forgejo_api_paginate "$url" "$token" "/api/v1/user/repos") || return 1

    # 2. Orgs the user belongs to
    local orgs
    orgs=$(_forgejo_api_paginate "$url" "$token" "/api/v1/user/orgs") || return 1

    local all_repos="$user_repos"
    local org
    while IFS= read -r org; do
        [[ -z "$org" ]] && continue
        local org_repos
        org_repos=$(_forgejo_api_paginate "$url" "$token" "/api/v1/orgs/${org}/repos") || continue
        all_repos=$(printf '%s\n%s' "$all_repos" "$org_repos" | jq -s 'add | unique_by(.full_name)')
    done < <(echo "$orgs" | jq -r '.[].username // .[].name // empty')

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
