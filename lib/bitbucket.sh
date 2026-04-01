#!/usr/bin/env bash
# Bitbucket provider (uses bitbucket-cli or curl + API)

bitbucket_login() {
    if command -v bitbucket &>/dev/null; then
        bitbucket auth login
    else
        log_info "No bitbucket CLI found. Using API token auth."
        log_info "Create an app password at: https://bitbucket.org/account/settings/app-passwords/"
        echo ""
        read -rp "Bitbucket username: " bb_user
        read -rsp "App password: " bb_pass
        echo ""

        local cred_dir="$HOME/.config/repos-manager"
        mkdir -p "$cred_dir"
        echo "${bb_user}:${bb_pass}" > "$cred_dir/bitbucket-creds"
        chmod 600 "$cred_dir/bitbucket-creds"
        log_success "Credentials saved to $cred_dir/bitbucket-creds"
    fi
}

bitbucket_list_repos() {
    if command -v bitbucket &>/dev/null; then
        # Use bitbucket-cli if available
        local raw
        raw=$(bitbucket repo list --output json --limit 1000 2>/dev/null || echo "[]")
        echo "$raw" | jq '[.[] | {
            nameWithOwner: .full_name,
            sshUrl: .links.clone[] | select(.name == "ssh") | .href,
            url: .links.clone[] | select(.name == "https") | .href
        }]' 2>/dev/null || echo "[]"
    else
        # Fallback to curl + API
        local cred_file="$HOME/.config/repos-manager/bitbucket-creds"
        if [[ ! -f "$cred_file" ]]; then
            log_error "Not authenticated. Run: repos-manager bitbucket login"
            return 1
        fi
        local creds
        creds=$(<"$cred_file")
        local user="${creds%%:*}"

        local all_repos="[]"
        local next_url="https://api.bitbucket.org/2.0/repositories/${user}?pagelen=100"

        while [[ -n "$next_url" && "$next_url" != "null" ]]; do
            local response
            response=$(curl -s -u "$creds" "$next_url")
            local page_repos
            page_repos=$(echo "$response" | jq '[.values[] | {
                nameWithOwner: .full_name,
                sshUrl: (.links.clone[] | select(.name == "ssh") | .href),
                url: (.links.clone[] | select(.name == "https") | .href)
            }]' 2>/dev/null || echo "[]")

            all_repos=$(printf '%s\n%s' "$all_repos" "$page_repos" | jq -s 'add | unique_by(.nameWithOwner)')
            next_url=$(echo "$response" | jq -r '.next // empty')
        done

        echo "$all_repos"
    fi
}

bitbucket_get_clone_url() {
    local repo_json="$1"
    if $USE_HTTPS; then
        echo "$repo_json" | jq -r '.url'
    else
        echo "$repo_json" | jq -r '.sshUrl'
    fi
}

bitbucket_get_full_name() {
    echo "$1" | jq -r '.nameWithOwner'
}
