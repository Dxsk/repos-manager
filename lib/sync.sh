#!/usr/bin/env bash
# Core sync engine

sync_provider() {
    local provider="$1" host="$2"

    log_info "Fetching repository list from ${host}..."

    local repos_json
    repos_json=$("${provider}_list_repos") || return 1

    local count
    count=$(echo "$repos_json" | jq 'length')
    log_info "Found ${count} repositories"
    echo

    local provider_dir="$BASE_DIR/$host"
    local cloned=0 updated=0 skipped=0 errored=0
    local -a synced_paths=()

    while IFS= read -r repo; do
        local full_name
        full_name=$("${provider}_get_full_name" "$repo")

        # Apply --filter flag
        if [[ -n "$FILTER" ]] && ! match_pattern "$FILTER" "$full_name"; then
            continue
        fi

        # Apply .repos-filter file
        if is_filtered_out "$full_name"; then
            continue
        fi

        # Apply .repos-ignore patterns
        if is_ignored "$full_name"; then
            log_skip "${full_name} (ignored)"
            skipped=$((skipped + 1))
            continue
        fi

        local local_path="$provider_dir/$full_name"
        synced_paths+=("$local_path")

        local clone_url
        clone_url=$("${provider}_get_clone_url" "$repo")

        if $DRY_RUN; then
            if [[ -d "$local_path/.git" ]]; then
                log_info "  [dry-run] would update ${full_name}"
            else
                log_info "  [dry-run] would clone ${full_name}"
            fi
            continue
        fi

        if [[ -d "$local_path/.git" ]]; then
            # Existing repo: check for uncommitted changes
            if [[ -n "$(git -C "$local_path" status --porcelain 2>/dev/null)" ]]; then
                log_warn "${full_name} (dirty, skipped)"
                skipped=$((skipped + 1))
            else
                local err
                if err=$(git -C "$local_path" fetch --all --quiet 2>&1) && \
                   err=$(git -C "$local_path" pull --ff-only --quiet 2>&1); then
                    log_success "${full_name} (updated)"
                    updated=$((updated + 1))
                else
                    log_error "${full_name} (update failed): ${err}"
                    errored=$((errored + 1))
                fi
            fi
        else
            # New repo: clone it
            mkdir -p "$(dirname "$local_path")"
            local err
            if err=$(git clone --quiet "$clone_url" "$local_path" 2>&1); then
                log_success "${full_name} (cloned)"
                cloned=$((cloned + 1))
            else
                log_error "${full_name} (clone failed): ${err}"
                errored=$((errored + 1))
            fi
        fi
    done < <(echo "$repos_json" | jq -c '.[]')

    # Prune repos no longer on remote
    if $PRUNE; then
        if [[ -n "$FILTER" ]]; then
            log_warn "Pruning skipped: not supported with --filter"
        else
            prune_repos "$provider_dir" "${synced_paths[@]+${synced_paths[@]}}"
        fi
    fi

    echo
    log_info "Done: ${cloned} cloned, ${updated} updated, ${skipped} skipped, ${errored} errors"
}

prune_repos() {
    local provider_dir="$1"
    shift
    local -a synced=("${@}")

    [[ ! -d "$provider_dir" ]] && return

    # Safety: ensure provider_dir is under BASE_DIR
    local real_provider
    real_provider=$(realpath "$provider_dir" 2>/dev/null) || return
    local real_base
    real_base=$(realpath "$BASE_DIR" 2>/dev/null) || return

    if [[ "$real_provider" != "$real_base"/* ]]; then
        log_error "Prune aborted: provider directory is outside BASE_DIR"
        return 1
    fi

    local pruned=0
    while IFS= read -r -d '' git_dir; do
        local repo_dir="${git_dir%/.git}"
        local found=false

        for s in "${synced[@]+${synced[@]}}"; do
            if [[ "$s" == "$repo_dir" ]]; then
                found=true
                break
            fi
        done

        if ! $found; then
            local rel_path="${repo_dir#"$BASE_DIR"/}"
            if $DRY_RUN; then
                log_warn "[dry-run] would prune ${rel_path}"
            else
                log_error "${rel_path} (pruned)"
                rm -rf "$repo_dir"
            fi
            pruned=$((pruned + 1))
        fi
    done < <(find "$provider_dir" -name ".git" -type d -print0 2>/dev/null)

    [[ $pruned -gt 0 ]] && log_warn "Pruned ${pruned} repositories"
    return 0
}
