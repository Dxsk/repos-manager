#!/usr/bin/env bash
# Background update check: fetch the latest published version at most
# once per TTL and, on subsequent runs, display a one-line banner when a
# newer version is available. The fetch runs detached so it never slows
# down the command in progress.

REPOS_MANAGER_VERSION_URL="${REPOS_MANAGER_VERSION_URL:-https://raw.githubusercontent.com/Dxsk/repos-manager/main/repos-manager.sh}"
REPOS_MANAGER_UPDATE_CACHE="${REPOS_MANAGER_UPDATE_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/repos-manager/latest-version}"
REPOS_MANAGER_UPDATE_TTL="${REPOS_MANAGER_UPDATE_TTL:-86400}"  # seconds

# Return 0 when update checks are enabled for the current invocation.
_update_check_enabled() {
    [[ "${REPOS_MANAGER_NO_UPDATE_CHECK:-0}" == "1" ]] && return 1
    [[ "${CHECK_UPDATES:-true}" == "false" ]] && return 1
    command -v curl &>/dev/null || return 1
    return 0
}

# Return 0 if the cache file is missing or older than the TTL.
_update_check_cache_stale() {
    [[ ! -f "$REPOS_MANAGER_UPDATE_CACHE" ]] && return 0
    local now mtime
    now=$(date +%s)
    # stat flags differ between GNU (-c) and BSD (-f)
    mtime=$(stat -c %Y "$REPOS_MANAGER_UPDATE_CACHE" 2>/dev/null \
         || stat -f %m "$REPOS_MANAGER_UPDATE_CACHE" 2>/dev/null \
         || echo 0)
    (( now - mtime >= REPOS_MANAGER_UPDATE_TTL ))
}

# Spawn a detached background job that refreshes the cached version.
# Never blocks the caller and swallows all output/errors.
update_check_refresh_async() {
    _update_check_enabled || return 0
    _update_check_cache_stale || return 0

    local cache_dir
    cache_dir=$(dirname "$REPOS_MANAGER_UPDATE_CACHE")
    mkdir -p "$cache_dir" 2>/dev/null || return 0

    (
        local body latest
        body=$(curl -fsSL --max-time 5 "$REPOS_MANAGER_VERSION_URL" 2>/dev/null) || exit 0
        latest=$(printf '%s\n' "$body" | grep -m1 '^VERSION=' | cut -d'"' -f2)
        [[ -n "$latest" ]] || exit 0
        printf '%s\n' "$latest" > "$REPOS_MANAGER_UPDATE_CACHE"
    ) </dev/null >/dev/null 2>&1 &
    disown 2>/dev/null || true
}

# Print a one-line banner to stderr when the cached version is newer
# than the running VERSION. Silent otherwise. Safe to call unconditionally.
update_check_banner() {
    _update_check_enabled || return 0
    [[ -f "$REPOS_MANAGER_UPDATE_CACHE" ]] || return 0

    local latest
    latest=$(head -n1 "$REPOS_MANAGER_UPDATE_CACHE" 2>/dev/null)
    [[ -z "$latest" ]] && return 0
    [[ "$latest" == "$VERSION" ]] && return 0

    # Only announce if latest sorts strictly greater than current.
    local newer
    newer=$(printf '%s\n%s\n' "$VERSION" "$latest" | sort -V | tail -n1)
    [[ "$newer" == "$VERSION" ]] && return 0

    printf "%s⬆ repos-manager %s available (current %s) — run: repos-manager update%s\n" \
        "$YELLOW" "$latest" "$VERSION" "$RESET" >&2
}
