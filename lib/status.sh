#!/usr/bin/env bash
# Status: show dirty/ahead/behind/diverged repos

status_all() {
    local total=0 dirty=0 ahead=0 behind=0 diverged=0 clean=0

    log_info "Scanning repos in ${BASE_DIR}..."
    echo

    # Show a live "scanned N: <path>" indicator while we walk the tree.
    # Only on a TTY and when not quiet, so logs/pipes stay clean.
    local show_progress=false
    if [[ -t 2 ]] && ! $QUIET; then
        show_progress=true
    fi

    while IFS= read -r -d '' git_dir; do
        local repo_dir="${git_dir%/.git}"
        local rel_path="${repo_dir#"$BASE_DIR"/}"
        total=$((total + 1))

        if $show_progress; then
            # \r + clear-to-end-of-line; truncate very long paths to 70 chars
            # so we do not wrap on narrow terminals.
            local display="$rel_path"
            if (( ${#display} > 70 )); then
                display="…${display: -69}"
            fi
            printf '\r  %s[%d]%s %s\033[K' "$GRAY" "$total" "$RESET" "$display" >&2
        fi

        local flags=""

        # Check dirty (uncommitted changes)
        if [[ -n "$(git -C "$repo_dir" status --porcelain 2>/dev/null)" ]]; then
            flags+="${YELLOW}dirty${RESET} "
            dirty=$((dirty + 1))
        fi

        # Check ahead/behind
        local upstream
        upstream=$(git -C "$repo_dir" rev-parse --abbrev-ref '@{upstream}' 2>/dev/null) || true

        if [[ -n "$upstream" ]]; then
            local ab
            ab=$(git -C "$repo_dir" rev-list --left-right --count HEAD..."$upstream" 2>/dev/null) || true

            if [[ -n "$ab" ]]; then
                local a b
                a=$(echo "$ab" | awk '{print $1}')
                b=$(echo "$ab" | awk '{print $2}')

                if [[ "$a" -gt 0 && "$b" -gt 0 ]]; then
                    flags+="${RED}diverged${RESET} (+${a}/-${b}) "
                    diverged=$((diverged + 1))
                elif [[ "$a" -gt 0 ]]; then
                    flags+="${GREEN}ahead${RESET} (+${a}) "
                    ahead=$((ahead + 1))
                elif [[ "$b" -gt 0 ]]; then
                    flags+="${BLUE}behind${RESET} (-${b}) "
                    behind=$((behind + 1))
                fi
            fi
        fi

        if [[ -n "$flags" ]]; then
            printf "  %s %s\n" "$rel_path" "$flags"
        else
            clean=$((clean + 1))
        fi
    done < <(
        # Prune heavy directories that never contain a tracked repo: huge
        # dependency trees, build outputs, and VCS-internal mirrors. This
        # keeps `status` fast on a BASE_DIR that also hosts working trees
        # with vendored deps.
        find "$BASE_DIR" \
            \( -type d \( \
                   -name node_modules \
                -o -name .venv \
                -o -name venv \
                -o -name __pycache__ \
                -o -name target \
                -o -name vendor \
                -o -name dist \
                -o -name build \
                -o -name .next \
                -o -name .cache \
            \) -prune \) \
            -o -name ".git" -type d -print0 2>/dev/null \
            | sort -z
    )

    if $show_progress; then
        printf '\r\033[K' >&2  # clear progress line
    fi

    echo
    log_info "Total: ${total} repos - ${GREEN}${clean} clean${RESET}, ${YELLOW}${dirty} dirty${RESET}, ${GREEN}${ahead} ahead${RESET}, ${BLUE}${behind} behind${RESET}, ${RED}${diverged} diverged${RESET}"
}
