#!/usr/bin/env bash
# Core sync engine

acquire_lock() {
    local dir="$1"
    local lockfile="$dir/.repos-manager.lock"

    mkdir -p "$dir" 2>/dev/null

    if [[ -f "$lockfile" ]]; then
        local pid
        pid=$(<"$lockfile")
        if kill -0 "$pid" 2>/dev/null; then
            log_error "Another sync is running (PID $pid). Remove $lockfile if stale."
            return 1
        fi
        # Stale lock from dead process
        rm -f "$lockfile"
    fi

    echo "$$" > "$lockfile"
}

release_lock() {
    local dir="$1"
    rm -f "$dir/.repos-manager.lock"
}

sync_repo() {
    local provider="$1" local_path="$2" clone_url="$3" full_name="$4"

    if [[ -d "$local_path/.git" ]]; then
        # Existing repo: check for uncommitted changes
        if [[ -n "$(git -C "$local_path" status --porcelain 2>/dev/null)" ]]; then
            log_warn "${full_name} (dirty, skipped)" >&2
            echo "skipped"
        else
            # Pull explicitly from origin's current branch so repos whose
            # local branch has no upstream tracking configured (e.g. manual
            # clones, renamed default branches) still update cleanly.
            local err
            if err=$(git -C "$local_path" fetch --all --quiet 2>&1) && \
               err=$(git -C "$local_path" pull --ff-only --quiet origin HEAD 2>&1); then
                log_success "${full_name} (updated)" >&2
                echo "updated"
            else
                log_error "${full_name} (update failed): ${err}" >&2
                echo "errored"
            fi
        fi
    else
        # New repo: clone it
        mkdir -p "$(dirname "$local_path")"
        local err
        if err=$(git clone --quiet "$clone_url" "$local_path" 2>&1); then
            log_success "${full_name} (cloned)" >&2
            echo "cloned"
        else
            log_error "${full_name} (clone failed): ${err}" >&2
            echo "errored"
        fi
    fi
}

sync_provider() {
    local provider="$1" host="$2"

    # Lock per-host, not on BASE_DIR: distinct providers/hosts write to
    # disjoint subtrees ($BASE_DIR/$host) and should be allowed to run
    # concurrently.
    local lock_dir="$BASE_DIR/$host"
    acquire_lock "$lock_dir" || return 1

    log_info "Fetching repository list from ${host}..."

    local repos_json rc=0
    repos_json=$("${provider}_list_repos") || rc=$?
    if [[ $rc -ne 0 ]]; then
        release_lock "$lock_dir"
        # rc=2 means "skip this host" (e.g. not logged in) — not a hard error
        if [[ $rc -eq 2 ]]; then
            log_warn "Skipping ${host} (not configured)"
            return 0
        fi
        return 1
    fi

    local count
    count=$(echo "$repos_json" | jq 'length')
    log_info "Found ${count} repositories"
    echo

    local provider_dir="$BASE_DIR/$host"
    local cloned=0 updated=0 skipped=0 errored=0
    local -a synced_paths=()
    local tmpdir
    tmpdir=$(mktemp -d)

    local running=0

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

        # Run sync in background with job limiting
        (
            result=$(sync_repo "$provider" "$local_path" "$clone_url" "$full_name")
            echo "$result" > "$tmpdir/$(echo "$full_name" | tr '/' '_')"
        ) &
        running=$((running + 1))

        if [[ $running -ge $PARALLEL ]]; then
            wait -n 2>/dev/null || true
            running=$((running - 1))
        fi
    done < <(echo "$repos_json" | jq -c '.[]')

    # Wait for remaining jobs
    wait

    # Count results
    for f in "$tmpdir"/*; do
        [[ -f "$f" ]] || continue
        case "$(cat "$f")" in
            cloned)  cloned=$((cloned + 1)) ;;
            updated) updated=$((updated + 1)) ;;
            skipped) skipped=$((skipped + 1)) ;;
            errored) errored=$((errored + 1)) ;;
        esac
    done
    rm -rf "$tmpdir"

    # Prune repos no longer on remote
    if $PRUNE; then
        if [[ -n "$FILTER" ]]; then
            log_warn "Pruning skipped: not supported with --filter"
        else
            prune_repos "$provider_dir" "${synced_paths[@]+${synced_paths[@]}}"
        fi
    fi

    release_lock "$lock_dir"

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
