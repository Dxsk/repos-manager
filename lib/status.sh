#!/usr/bin/env bash
# Status: show dirty/ahead/behind/diverged repos

status_all() {
    local total=0 dirty=0 ahead=0 behind=0 diverged=0 clean=0

    log_info "Scanning repos in ${BASE_DIR}..."
    echo

    while IFS= read -r -d '' git_dir; do
        local repo_dir="${git_dir%/.git}"
        local rel_path="${repo_dir#"$BASE_DIR"/}"
        total=$((total + 1))

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
    done < <(find "$BASE_DIR" -name ".git" -type d -print0 2>/dev/null | sort -z)

    echo
    log_info "Total: ${total} repos - ${GREEN}${clean} clean${RESET}, ${YELLOW}${dirty} dirty${RESET}, ${GREEN}${ahead} ahead${RESET}, ${BLUE}${behind} behind${RESET}, ${RED}${diverged} diverged${RESET}"
}
