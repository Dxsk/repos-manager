#!/usr/bin/env bash
# Status: show dirty/ahead/behind/diverged repos

# Filesystems backed by network or userspace drivers that almost always
# make recursive walks painfully slow (webdav, cloud sync, NFS, SMB, ssh
# fuse, etc). A single .git directory on such a mount can contain
# hundreds of tiny objects, each translating to a remote round-trip, and
# in practice hangs `find` for minutes. We skip them by default.
_STATUS_NETWORK_FSTYPES='^(fuse([.].*)?|nfs[0-9]*|cifs|smb[0-9]*|smbfs|afs|ceph|davfs)$'

# Read /proc/self/mountinfo (or a path provided as $1 for testing) and
# emit one mount point per line for every entry whose filesystem type
# matches the network pattern above and whose mount point is under
# BASE_DIR. Portable to any mountinfo-compatible source.
# shellcheck disable=SC2120  # $1 is an optional override used by tests
_status_network_mount_points() {
    local source="${1:-/proc/self/mountinfo}"
    [[ -r "$source" ]] || return 0
    [[ -n "${BASE_DIR:-}" ]] || return 0

    # mountinfo fields (space-separated):
    #   1 mount_id  2 parent_id  3 major:minor  4 root  5 mount_point
    #   6 options   7.. optional_fields  -  fstype  source  super_options
    # We need field 5 (mount point) and the field after the literal "-".
    # IMPORTANT — bash 3.2 parser quirk (macOS ships bash 3.2 by default):
    # bash 3.2 scans for $( sequences greedily, even inside single-quoted
    # strings, and will mis-parse an awk script like $(i+1) inside this
    # here-script as a shell command substitution with an unclosed
    # parenthesis. The visible symptom is a 'bad substitution: no closing
    # )' in <(' error that aborts sourcing of status.sh, which then makes
    # every status test fail on the macOS GitHub runner. The workaround
    # is to hoist any awk field index into a plain variable first (`j`
    # below) and reference it as $j rather than $(i+1). The project
    # requires bash 4+ at runtime, but the test CI intentionally runs
    # under whatever bash is on the runner so regressions like this
    # surface early. See commit c3a7bee for the original diagnosis.
    awk -v base="$BASE_DIR" -v re="$_STATUS_NETWORK_FSTYPES" '
        {
            mp = $5
            # Find the "-" separator, fstype follows immediately after.
            fstype = ""
            for (i = 6; i <= NF; i++) {
                if ($i == "-") {
                    j = i + 1
                    fstype = $j
                    break
                }
            }
            if (fstype ~ re && index(mp, base "/") == 1) {
                print mp
            }
        }
    ' "$source"
}

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

    # Build a list of extra -path prune arguments for network/FUSE mount
    # points under BASE_DIR, unless the user explicitly opted in to
    # scanning them. Cloud drives (kDrive, Dropbox via FUSE, sshfs, NFS…)
    # are the #1 cause of apparent hangs on `status`. This must run
    # before the while loop, as the process substitution below snapshots
    # the array at invocation time.
    local -a extra_prune=()
    if [[ "${SCAN_NETWORK_MOUNTS:-false}" != "true" ]]; then
        local mp
        while IFS= read -r mp; do
            [[ -z "$mp" ]] && continue
            extra_prune+=(-o -path "$mp" -prune)
            log_debug "status: pruning network mount ${mp}"
        done < <(_status_network_mount_points)
    else
        log_warn "status: scanning network mounts (can be very slow or hang on unreliable links)"
    fi

    # IMPORTANT - bash 3.2 parser quirk (macOS default shell):
    # bash 3.2 mis-parses `${arr[@]+"${arr[@]}"}` when it appears
    # inside a `< <( ... )` process substitution, emitting
    # `bad substitution: no closing ')' in <(`. Build the find
    # invocation into an array BEFORE the while loop so the process
    # substitution only contains a simple array expansion, and expand
    # the optional prune elements with a plain length check instead
    # of the `${+}` form. See contributing doc, "Bash portability
    # notes".
    local -a find_cmd
    find_cmd=(
        find "$BASE_DIR"
        \( -type d \(
               -name node_modules
            -o -name .venv
            -o -name venv
            -o -name __pycache__
            -o -name target
            -o -name vendor
            -o -name dist
            -o -name build
            -o -name .next
            -o -name .cache
        \) -prune \)
    )
    if (( ${#extra_prune[@]} > 0 )); then
        find_cmd+=("${extra_prune[@]}")
    fi
    find_cmd+=(-o -name ".git" -type d -print0)

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
            # Clear the in-place progress line before emitting a
            # permanent status line so they do not concatenate.
            $show_progress && printf '\r\033[K' >&2
            printf "  %s %s\n" "$rel_path" "$flags"
        else
            clean=$((clean + 1))
        fi
    done < <("${find_cmd[@]}" 2>/dev/null)

    if $show_progress; then
        printf '\r\033[K' >&2  # clear progress line
    fi

    echo
    log_info "Total: ${total} repos - ${GREEN}${clean} clean${RESET}, ${YELLOW}${dirty} dirty${RESET}, ${GREEN}${ahead} ahead${RESET}, ${BLUE}${behind} behind${RESET}, ${RED}${diverged} diverged${RESET}"
}
