#!/usr/bin/env bash
# Configuration file support
# Reads ~/.config/repos-manager/config.json
# shellcheck disable=SC2034  # Variables used in other sourced files

REPOS_MANAGER_CONFIG="${REPOS_MANAGER_CONFIG:-$HOME/.config/repos-manager/config.json}"

# Per-provider host lists, populated by load_config / defaults_hosts.
# Each variable is a bash array of hostnames. Empty means "skip provider".
HOSTS_GITHUB=()
HOSTS_GITLAB=()
HOSTS_FORGEJO=()
HOSTS_BITBUCKET=()
HOSTS_RADICLE=()

# Read .hosts.<provider> from the config file as a list, normalizing
# both legacy string form ("hosts.gitlab": "gitlab.com") and new array
# form ("hosts.gitlab": ["gitlab.com", "gitlab.example.org"]).
_load_hosts_for() {
    local provider="$1"
    jq -r --arg p "$provider" '
        .hosts[$p]
        | if . == null then empty
          elif type == "string" then .
          elif type == "array" then .[]
          else empty end
    ' "$REPOS_MANAGER_CONFIG" 2>/dev/null || true
}

_default_hosts() {
    HOSTS_GITHUB=("github.com")
    HOSTS_GITLAB=("gitlab.com")
    HOSTS_FORGEJO=("codeberg.org")
    HOSTS_BITBUCKET=("bitbucket.org")
    HOSTS_RADICLE=("radicle")
}

load_config() {
    _default_hosts

    [[ ! -f "$REPOS_MANAGER_CONFIG" ]] && return 0

    local val

    # Base directory (only override if no env var was set)
    if [[ -z "${REPOS_MANAGER_BASE_DIR:-}" ]]; then
        val=$(jq -r '.base_dir // empty' "$REPOS_MANAGER_CONFIG" 2>/dev/null || true)
        if [[ -n "$val" ]]; then
            BASE_DIR="${val/#\~/$HOME}"
        fi
    fi

    # Default parallel jobs (only if not already set by env)
    if [[ -z "${REPOS_MANAGER_PARALLEL:-}" ]]; then
        val=$(jq -r '.parallel // empty' "$REPOS_MANAGER_CONFIG" 2>/dev/null || true)
        if [[ -n "$val" ]]; then
            PARALLEL="$val"
        fi
    fi

    # Default protocol (only if not already set by env)
    if [[ -z "${REPOS_MANAGER_PROTOCOL:-}" ]]; then
        val=$(jq -r '.protocol // empty' "$REPOS_MANAGER_CONFIG" 2>/dev/null || true)
        if [[ "$val" == "https" ]]; then
            USE_HTTPS=true
        fi
    fi

    # Per-provider host lists. Only override the defaults if the user
    # actually declared hosts for that provider in the config.
    # NB: macOS ships bash 3.2 which has no `mapfile`, so read in a loop.
    local -a tmp
    local p line
    for p in github gitlab forgejo bitbucket radicle; do
        tmp=()
        while IFS= read -r line; do
            [[ -n "$line" ]] && tmp+=("$line")
        done < <(_load_hosts_for "$p")
        if [[ ${#tmp[@]} -gt 0 ]]; then
            case "$p" in
                github)    HOSTS_GITHUB=("${tmp[@]}") ;;
                gitlab)    HOSTS_GITLAB=("${tmp[@]}") ;;
                forgejo)   HOSTS_FORGEJO=("${tmp[@]}") ;;
                bitbucket) HOSTS_BITBUCKET=("${tmp[@]}") ;;
                radicle)   HOSTS_RADICLE=("${tmp[@]}") ;;
            esac
        fi
    done

    return 0
}

# Echo the configured hosts for a provider, one per line.
provider_hosts() {
    local provider="$1"
    case "$provider" in
        github)    printf '%s\n' "${HOSTS_GITHUB[@]+${HOSTS_GITHUB[@]}}" ;;
        gitlab)    printf '%s\n' "${HOSTS_GITLAB[@]+${HOSTS_GITLAB[@]}}" ;;
        forgejo)   printf '%s\n' "${HOSTS_FORGEJO[@]+${HOSTS_FORGEJO[@]}}" ;;
        bitbucket) printf '%s\n' "${HOSTS_BITBUCKET[@]+${HOSTS_BITBUCKET[@]}}" ;;
        radicle)   printf '%s\n' "${HOSTS_RADICLE[@]+${HOSTS_RADICLE[@]}}" ;;
    esac
}

init_config() {
    local config_dir
    config_dir=$(dirname "$REPOS_MANAGER_CONFIG")
    mkdir -p "$config_dir"
    chmod 700 "$config_dir"

    if [[ -f "$REPOS_MANAGER_CONFIG" ]]; then
        log_warn "Config already exists: $REPOS_MANAGER_CONFIG"
        return 0
    fi

    cat > "$REPOS_MANAGER_CONFIG" <<'JSON'
{
  "base_dir": "~/Documents",
  "parallel": 4,
  "protocol": "ssh",
  "hosts": {
    "github":    ["github.com"],
    "gitlab":    ["gitlab.com"],
    "forgejo":   ["codeberg.org"],
    "bitbucket": ["bitbucket.org"]
  }
}
JSON

    log_success "Config created: $REPOS_MANAGER_CONFIG"
}

generate_sourceme() {
    local host_dir="$1"

    cat > "$host_dir/sourceme" <<'SOURCEME'
#!/bin/bash

repos-manager() {
    bash "$HOME/.local/bin/repos-manager" "$@"
}

_repos_manager_complete() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    local prev="${COMP_WORDS[COMP_CWORD-1]}"

    case "$COMP_CWORD" in
        1)
            mapfile -t COMPREPLY < <(compgen -W "github gitlab forgejo gitea bitbucket radicle login sync status init update version help" -- "$cur")
            ;;
        2)
            case "$prev" in
                github|gitlab|forgejo|gitea|bitbucket|radicle)
                    mapfile -t COMPREPLY < <(compgen -W "login sync help" -- "$cur")
                    ;;
                sync)
                    mapfile -t COMPREPLY < <(compgen -W "--all --filter --base-dir --https --prune --dry-run --host --parallel --verbose --quiet" -- "$cur")
                    ;;
            esac
            ;;
        *)
            mapfile -t COMPREPLY < <(compgen -W "--filter --base-dir --https --prune --dry-run --host --parallel --verbose --quiet" -- "$cur")
            ;;
    esac
}
complete -F _repos_manager_complete repos-manager
SOURCEME

    cat > "$host_dir/sourceme.zsh" <<'SOURCEME'
_REPOS_MANAGER_ROOT="$(cd "$(dirname "${(%):-%x}")" && pwd)"

repos-manager() {
    bash "$HOME/.local/bin/repos-manager" "$@"
}

_repos_manager_complete_zsh() {
    local -a providers=("github:GitHub" "gitlab:GitLab" "forgejo:Forgejo/Gitea" "gitea:Alias for forgejo" "bitbucket:Bitbucket" "radicle:Radicle")
    local -a commands=("login:Authenticate" "sync:Sync repositories" "help:Show help")
    local -a global_cmds=("login:Authenticate" "sync:Sync all" "status:Show status" "init:Create config" "update:Self-update" "version:Show version" "help:Show help")
    local -a flags=(
        "--filter:Filter repos by pattern"
        "--base-dir:Base directory"
        "--https:Use HTTPS"
        "--prune:Remove stale repos"
        "--dry-run:Preview changes"
        "--host:Custom host"
        "--parallel:Parallel jobs"
        "--verbose:Debug output"
        "--quiet:Suppress output"
    )

    case "$CURRENT" in
        2)
            _describe 'provider' providers
            _describe 'command' global_cmds
            ;;
        3)
            case "${words[2]}" in
                github|gitlab|forgejo|gitea|bitbucket|radicle)
                    _describe 'command' commands
                    ;;
                sync)
                    _describe 'flag' flags
                    ;;
            esac
            ;;
        *)
            _describe 'flag' flags
            ;;
    esac
}
compdef _repos_manager_complete_zsh repos-manager
SOURCEME

    cat > "$host_dir/sourceme.fish" <<'SOURCEME'
function repos-manager
    bash "$HOME/.local/bin/repos-manager" $argv
end

complete -c repos-manager -f
complete -c repos-manager -n "__fish_use_subcommand" -a "github"    -d "GitHub"
complete -c repos-manager -n "__fish_use_subcommand" -a "gitlab"    -d "GitLab"
complete -c repos-manager -n "__fish_use_subcommand" -a "forgejo"   -d "Forgejo/Gitea"
complete -c repos-manager -n "__fish_use_subcommand" -a "gitea"     -d "Alias for forgejo"
complete -c repos-manager -n "__fish_use_subcommand" -a "bitbucket" -d "Bitbucket"
complete -c repos-manager -n "__fish_use_subcommand" -a "radicle"   -d "Radicle"
complete -c repos-manager -n "__fish_use_subcommand" -a "login"     -d "Authenticate"
complete -c repos-manager -n "__fish_use_subcommand" -a "sync"      -d "Sync all"
complete -c repos-manager -n "__fish_use_subcommand" -a "status"    -d "Show status"
complete -c repos-manager -n "__fish_use_subcommand" -a "init"      -d "Create config"
complete -c repos-manager -n "__fish_use_subcommand" -a "update"    -d "Self-update"
complete -c repos-manager -n "__fish_use_subcommand" -a "version"   -d "Show version"
complete -c repos-manager -n "__fish_use_subcommand" -a "help"      -d "Show help"
complete -c repos-manager -n "__fish_seen_subcommand_from github gitlab forgejo gitea bitbucket radicle" -a "login" -d "Authenticate"
complete -c repos-manager -n "__fish_seen_subcommand_from github gitlab forgejo gitea bitbucket radicle" -a "sync"  -d "Sync repos"
complete -c repos-manager -n "__fish_seen_subcommand_from github gitlab forgejo gitea bitbucket radicle" -a "help"  -d "Show help"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l all      -d "Sync all providers"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l filter   -d "Filter pattern" -r
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l base-dir -d "Base directory" -r -F
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l https    -d "Use HTTPS"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l prune    -d "Remove stale repos"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l dry-run  -d "Preview changes"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l host     -d "Custom host" -r
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l parallel -d "Parallel jobs" -r
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l verbose  -d "Debug output"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -s v        -d "Debug output"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l quiet    -d "Suppress output"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -s q        -d "Suppress output"
SOURCEME
}
