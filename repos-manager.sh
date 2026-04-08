#!/usr/bin/env bash
set -euo pipefail

VERSION="0.5.0"
BASE_DIR="${REPOS_MANAGER_BASE_DIR:-$HOME/Documents}"

# Resolve lib directory (overridable for packaging)
REPOS_MANAGER_LIB="${REPOS_MANAGER_LIB:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib}"

# ── Source modules ──────────────────────────────────────────────────────────────

# shellcheck source=lib/log.sh
source "${REPOS_MANAGER_LIB}/log.sh"
# shellcheck source=lib/flags.sh
source "${REPOS_MANAGER_LIB}/flags.sh"
# shellcheck source=lib/config.sh
source "${REPOS_MANAGER_LIB}/config.sh"
# shellcheck source=lib/match.sh
source "${REPOS_MANAGER_LIB}/match.sh"
# shellcheck source=lib/github.sh
source "${REPOS_MANAGER_LIB}/github.sh"
# shellcheck source=lib/gitlab.sh
source "${REPOS_MANAGER_LIB}/gitlab.sh"
# shellcheck source=lib/forgejo.sh
source "${REPOS_MANAGER_LIB}/forgejo.sh"
# shellcheck source=lib/bitbucket.sh
source "${REPOS_MANAGER_LIB}/bitbucket.sh"
# shellcheck source=lib/radicle.sh
source "${REPOS_MANAGER_LIB}/radicle.sh"
# shellcheck source=lib/sync.sh
source "${REPOS_MANAGER_LIB}/sync.sh"
# shellcheck source=lib/status.sh
source "${REPOS_MANAGER_LIB}/status.sh"
# shellcheck source=lib/update.sh
source "${REPOS_MANAGER_LIB}/update.sh"
# shellcheck source=lib/update_check.sh
source "${REPOS_MANAGER_LIB}/update_check.sh"

# ── Load config ─────────────────────────────────────────────────────────────────

load_config

# ── Dependency check ────────────────────────────────────────────────────────────

check_deps() {
    local missing=()
    for cmd in git jq; do
        command -v "$cmd" &>/dev/null || missing+=("$cmd")
    done
    if [[ ${#missing[@]} -gt 0 ]]; then
        echo "Missing required dependencies: ${missing[*]}" >&2
        exit 1
    fi
}

validate_base_dir() {
    if [[ -z "$BASE_DIR" ]]; then
        echo "BASE_DIR is empty" >&2
        exit 1
    fi
    if [[ "$BASE_DIR" != /* ]]; then
        echo "BASE_DIR must be an absolute path: $BASE_DIR" >&2
        exit 1
    fi
    mkdir -p "$BASE_DIR" 2>/dev/null || {
        echo "Cannot create BASE_DIR: $BASE_DIR" >&2
        exit 1
    }
}

# ── Provider detection ─────────────────────────────────────────────────────────

detect_providers() {
    local -a found=()
    command -v gh &>/dev/null && found+=("github")
    command -v glab &>/dev/null && found+=("gitlab")
    command -v tea &>/dev/null && found+=("forgejo")
    command -v bitbucket &>/dev/null && found+=("bitbucket")
    [[ -f "$HOME/.config/repos-manager/bitbucket-creds" ]] && [[ ! " ${found[*]} " =~ " bitbucket " ]] && found+=("bitbucket")
    command -v rad &>/dev/null && found+=("radicle")
    echo "${found[@]}"
}

# ── Commands ────────────────────────────────────────────────────────────────────

readonly VALID_PROVIDERS="github gitlab forgejo bitbucket radicle"

validate_provider() {
    local provider="$1"
    local p
    for p in $VALID_PROVIDERS; do
        [[ "$p" == "$provider" ]] && return 0
    done
    echo "Invalid provider: $provider" >&2
    exit 1
}

cmd_login() {
    local provider="${1:-}"

    if [[ -z "$provider" ]]; then
        local providers
        providers=$(detect_providers)
        if [[ -z "$providers" ]]; then
            log_error "No provider CLIs found (gh, glab, tea, bitbucket, rad)"
            exit 1
        fi
        for p in $providers; do
            printf "\n${BOLD}=== Login %s ===${RESET}\n\n" "$p"
            "${p}_login" || true
        done
    else
        validate_provider "$provider"
        "${provider}_login"
    fi
}

_provider_cli() {
    case "$1" in
        github)    echo "gh" ;;
        gitlab)    echo "glab" ;;
        forgejo)   echo "tea" ;;
        bitbucket) echo "bitbucket" ;;
        radicle)   echo "rad" ;;
    esac
}

# Sync one (provider, host) pair: runs sync_provider with CURRENT_HOST set so
# provider modules can target the right backend, then regenerates sourceme.
_sync_one() {
    local provider="$1" host="$2"
    CURRENT_HOST="$host" sync_provider "$provider" "$host"

    local host_dir="$BASE_DIR/$host"
    if [[ -d "$host_dir" ]]; then
        generate_sourceme "$host_dir"
    fi
}

cmd_sync() {
    local provider="$1"
    validate_provider "$provider"
    shift
    parse_flags "$@"

    local -a hosts
    if [[ -n "$HOST" ]]; then
        # --host flag forces a single host, overriding config
        hosts=("$HOST")
    else
        hosts=()
        while IFS= read -r _h; do
            [[ -n "$_h" ]] && hosts+=("$_h")
        done < <(provider_hosts "$provider")
    fi

    if [[ ${#hosts[@]} -eq 0 ]]; then
        log_error "No hosts configured for provider: $provider"
        exit 1
    fi

    local host
    for host in "${hosts[@]}"; do
        if [[ ${#hosts[@]} -gt 1 ]]; then
            printf "\n${BOLD}--- %s @ %s ---${RESET}\n\n" "$provider" "$host"
        fi
        _sync_one "$provider" "$host"
    done
}

cmd_sync_all() {
    parse_flags "$@"

    local provider
    for provider in github gitlab forgejo bitbucket radicle; do
        local cli
        cli=$(_provider_cli "$provider")

        if ! command -v "$cli" &>/dev/null; then
            # Bitbucket fallback: check for API creds
            if [[ "$provider" == "bitbucket" && -f "$HOME/.config/repos-manager/bitbucket-creds" ]]; then
                : # proceed with API fallback
            else
                continue
            fi
        fi

        local -a hosts
        hosts=()
        while IFS= read -r _h; do
            [[ -n "$_h" ]] && hosts+=("$_h")
        done < <(provider_hosts "$provider")
        [[ ${#hosts[@]} -eq 0 ]] && continue

        local host
        for host in "${hosts[@]}"; do
            printf "\n${BOLD}=== Syncing %s @ %s ===${RESET}\n\n" "$provider" "$host"
            _sync_one "$provider" "$host" || true
        done
    done
}

cmd_status() {
    parse_flags "$@"
    status_all
}

cmd_init() {
    init_config
}

# ── Usage ───────────────────────────────────────────────────────────────────────

print_usage() {
    cat <<EOF
${BOLD}repos-manager${RESET} - Multi-provider Git repository manager

${BOLD}Usage:${RESET}
  repos-manager <provider> <command> [flags]
  repos-manager <command> [flags]

${BOLD}Providers:${RESET}
  github      GitHub (uses gh CLI)
  gitlab      GitLab (uses glab CLI)
  forgejo     Forgejo / Gitea (uses tea CLI)
  gitea       Alias for forgejo
  bitbucket   Bitbucket (uses bitbucket CLI or API)
  radicle     Radicle (uses rad CLI)

${BOLD}Commands:${RESET}
  login [provider]   Authenticate (all detected providers if none specified)
  sync --all         Sync all providers
  status             Show dirty/ahead/behind repos across all providers
  init               Create default config file
  update             Check for updates and self-update

${BOLD}Provider commands:${RESET}
  <provider> login   Authenticate with a specific provider
  <provider> sync    Sync repositories from a provider

${BOLD}Flags:${RESET}
  --filter <pattern>   Filter repos by pattern (e.g., Dxsk/* or Dxsk/project)
  --base-dir <path>    Base directory (default: ~/Documents)
  --https              Use HTTPS instead of SSH
  --prune              Remove local repos not on remote
  --dry-run            Show what would be done without making changes
  --host <host>        Custom host (for self-hosted GitLab/Forgejo)
  --parallel <n>       Number of parallel sync jobs (default: 4)
  --verbose, -v        Show debug output
  --quiet, -q          Suppress info/success messages (errors still shown)

${BOLD}Config:${RESET}
  ~/.config/repos-manager/config.json

${BOLD}Examples:${RESET}
  repos-manager init
  repos-manager login
  repos-manager github sync
  repos-manager sync --all --parallel 8
  repos-manager status
EOF
}

print_provider_usage() {
    local provider="$1"
    local cli host
    case "$provider" in
        github)    cli="gh";        host="github.com" ;;
        gitlab)    cli="glab";      host="${HOST:-gitlab.com}" ;;
        forgejo)   cli="tea";       host="${HOST:-gitea.com}" ;;
        bitbucket) cli="bitbucket"; host="${HOST:-bitbucket.org}" ;;
        radicle)   cli="rad";       host="radicle" ;;
    esac

    cat <<EOF
${BOLD}repos-manager ${provider}${RESET} - Manage ${provider} repositories

${BOLD}Usage:${RESET}
  repos-manager ${provider} <command> [flags]

${BOLD}Commands:${RESET}
  login   Authenticate with ${provider} (requires ${cli} CLI)
  sync    Sync repositories from ${host}

${BOLD}Flags:${RESET}
  --filter <pattern>   Filter repos by pattern
  --https              Use HTTPS instead of SSH
  --prune              Remove local repos not on remote
  --dry-run            Preview without making changes
  --host <host>        Custom host (self-hosted instances)
  --parallel <n>       Parallel sync jobs (default: 4)
  --verbose, -v        Show debug output
  --quiet, -q          Suppress info/success messages

${BOLD}Examples:${RESET}
  repos-manager ${provider} login
  repos-manager ${provider} sync
  repos-manager ${provider} sync --filter owner/*
  repos-manager ${provider} sync --prune --parallel 8
EOF
}

# ── Entry point ─────────────────────────────────────────────────────────────────

main() {
    check_deps
    validate_base_dir

    # Non-blocking update check: show a banner based on the cached result
    # of a previous run, then refresh the cache in the background for next
    # time. Skipped for commands where the banner is noise or redundant.
    case "${1:-}" in
        update|version|--version|help|--help|-h|"") ;;
        *)
            update_check_banner
            update_check_refresh_async
            ;;
    esac

    case "${1:-}" in
        github)
            shift
            case "${1:-}" in
                login) cmd_login "github" ;;
                sync)  shift; cmd_sync "github" "$@" ;;
                help|--help|-h|"") print_provider_usage "github" ;;
                *)     print_provider_usage "github"; exit 1 ;;
            esac
            ;;
        gitlab)
            shift
            case "${1:-}" in
                login) cmd_login "gitlab" ;;
                sync)  shift; cmd_sync "gitlab" "$@" ;;
                help|--help|-h|"") print_provider_usage "gitlab" ;;
                *)     print_provider_usage "gitlab"; exit 1 ;;
            esac
            ;;
        forgejo|gitea)
            shift
            case "${1:-}" in
                login) cmd_login "forgejo" ;;
                sync)  shift; cmd_sync "forgejo" "$@" ;;
                help|--help|-h|"") print_provider_usage "forgejo" ;;
                *)     print_provider_usage "forgejo"; exit 1 ;;
            esac
            ;;
        bitbucket)
            shift
            case "${1:-}" in
                login) cmd_login "bitbucket" ;;
                sync)  shift; cmd_sync "bitbucket" "$@" ;;
                help|--help|-h|"") print_provider_usage "bitbucket" ;;
                *)     print_provider_usage "bitbucket"; exit 1 ;;
            esac
            ;;
        radicle)
            shift
            case "${1:-}" in
                login) cmd_login "radicle" ;;
                sync)  shift; cmd_sync "radicle" "$@" ;;
                help|--help|-h|"") print_provider_usage "radicle" ;;
                *)     print_provider_usage "radicle"; exit 1 ;;
            esac
            ;;
        login)
            shift
            cmd_login "${1:-}"
            ;;
        sync)
            shift
            if [[ "${1:-}" != "--all" ]]; then
                echo "Usage: repos-manager sync --all [flags]" >&2
                exit 1
            fi
            cmd_sync_all "$@"
            ;;
        status)
            shift
            cmd_status "$@"
            ;;
        init)
            cmd_init
            ;;
        update)
            self_update
            ;;
        version|--version)
            echo "repos-manager ${VERSION}"
            ;;
        help|--help|-h|"")
            print_usage
            ;;
        *)
            echo "Unknown command: $1" >&2
            print_usage
            exit 1
            ;;
    esac
}

main "$@"
