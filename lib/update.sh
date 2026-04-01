#!/usr/bin/env bash
# Self-update: check for new version and update if needed

REPOS_MANAGER_REPO="https://github.com/Dxsk/repos-manager.git"

self_update() {
    local script_dir
    script_dir=$(cd "$(dirname "${BASH_SOURCE[1]}")" && pwd)

    # Only works if repos-manager is a git repo (not installed via make)
    if [[ ! -d "$script_dir/.git" ]]; then
        # Installed via make — check against remote
        local tmp
        tmp=$(mktemp -d)
        if git clone --depth 1 --quiet "$REPOS_MANAGER_REPO" "$tmp" 2>/dev/null; then
            local remote_version
            remote_version=$(grep '^VERSION=' "$tmp/repos-manager.sh" | head -1 | cut -d'"' -f2)
            if [[ "$remote_version" != "$VERSION" ]]; then
                log_info "New version available: ${GREEN}${remote_version}${RESET} (current: ${VERSION})"
                read -rp "Update now? [y/N] " answer
                if [[ "$answer" == [yY] ]]; then
                    (cd "$tmp" && make install)
                    log_success "Updated to ${remote_version}. Restart repos-manager to use new version."
                fi
            else
                log_success "Already up to date (${VERSION})"
            fi
        else
            log_error "Failed to check for updates"
        fi
        rm -rf "$tmp"
        return
    fi

    # Running from git repo
    git -C "$script_dir" fetch --quiet origin main 2>/dev/null || {
        log_error "Failed to fetch updates"
        return 1
    }

    local local_hash remote_hash
    local_hash=$(git -C "$script_dir" rev-parse HEAD 2>/dev/null)
    remote_hash=$(git -C "$script_dir" rev-parse origin/main 2>/dev/null)

    if [[ "$local_hash" == "$remote_hash" ]]; then
        log_success "Already up to date (${VERSION})"
        return
    fi

    local behind
    behind=$(git -C "$script_dir" rev-list --count HEAD..origin/main 2>/dev/null)
    log_info "Update available: ${GREEN}${behind} commit(s)${RESET} behind origin/main"

    # Show what changed
    git -C "$script_dir" --no-pager log --oneline HEAD..origin/main 2>/dev/null | while read -r line; do
        echo "  ${GRAY}${line}${RESET}"
    done
    echo

    read -rp "Update now? [y/N] " answer
    if [[ "$answer" != [yY] ]]; then
        return
    fi

    # Check for local changes
    if [[ -n "$(git -C "$script_dir" status --porcelain 2>/dev/null)" ]]; then
        log_warn "You have local changes. Stashing..."
        git -C "$script_dir" stash --quiet
        local stashed=true
    fi

    if git -C "$script_dir" pull --ff-only --quiet origin main 2>/dev/null; then
        local new_version
        new_version=$(grep '^VERSION=' "$script_dir/repos-manager.sh" | head -1 | cut -d'"' -f2)
        log_success "Updated to ${new_version}"
    else
        log_error "Update failed (non-fast-forward). Run manually: git -C $script_dir pull"
    fi

    if [[ "${stashed:-false}" == true ]]; then
        git -C "$script_dir" stash pop --quiet 2>/dev/null || log_warn "Failed to restore stash"
    fi
}
