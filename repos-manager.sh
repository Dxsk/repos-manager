#!/usr/bin/env bash
set -euo pipefail

VERSION="0.2.0"
BASE_DIR="${REPOS_MANAGER_BASE_DIR:-$HOME/Documents}"

# Resolve lib directory (overridable for packaging)
REPOS_MANAGER_LIB="${REPOS_MANAGER_LIB:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib}"

# ── Source modules ──────────────────────────────────────────────────────────────

# shellcheck source=lib/log.sh
source "${REPOS_MANAGER_LIB}/log.sh"
# shellcheck source=lib/flags.sh
source "${REPOS_MANAGER_LIB}/flags.sh"
# shellcheck source=lib/match.sh
source "${REPOS_MANAGER_LIB}/match.sh"
# shellcheck source=lib/github.sh
source "${REPOS_MANAGER_LIB}/github.sh"
# shellcheck source=lib/gitlab.sh
source "${REPOS_MANAGER_LIB}/gitlab.sh"
# shellcheck source=lib/forgejo.sh
source "${REPOS_MANAGER_LIB}/forgejo.sh"
# shellcheck source=lib/sync.sh
source "${REPOS_MANAGER_LIB}/sync.sh"
# shellcheck source=lib/status.sh
source "${REPOS_MANAGER_LIB}/status.sh"

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
    echo "${found[@]}"
}

# ── Commands ────────────────────────────────────────────────────────────────────

readonly VALID_PROVIDERS="github gitlab forgejo"

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
        # Login all detected providers
        local providers
        providers=$(detect_providers)
        if [[ -z "$providers" ]]; then
            log_error "No provider CLIs found (gh, glab, tea)"
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

cmd_sync() {
    local provider="$1"
    validate_provider "$provider"
    shift
    parse_flags "$@"

    local host
    case "$provider" in
        github)  host="github.com" ;;
        gitlab)  host="${HOST:-gitlab.com}" ;;
        forgejo) host="${HOST:-gitea.com}" ;;
        *) echo "Unknown provider: $provider" >&2; exit 1 ;;
    esac

    sync_provider "$provider" "$host"
}

cmd_sync_all() {
    parse_flags "$@"

    local -a providers=("github:github.com" "gitlab:${HOST:-gitlab.com}" "forgejo:${HOST:-gitea.com}")

    for entry in "${providers[@]}"; do
        local provider="${entry%%:*}"
        local host="${entry##*:}"

        local cli
        case "$provider" in
            github)  cli="gh" ;;
            gitlab)  cli="glab" ;;
            forgejo) cli="tea" ;;
        esac

        if ! command -v "$cli" &>/dev/null; then
            log_warn "Skipping ${provider}: ${cli} not found"
            continue
        fi

        printf "\n${BOLD}=== Syncing %s ===${RESET}\n\n" "$provider"
        sync_provider "$provider" "$host" || true
    done
}

cmd_status() {
    parse_flags "$@"
    status_all
}

# ── Usage ───────────────────────────────────────────────────────────────────────

print_usage() {
    cat <<EOF
${BOLD}repos-manager${RESET} - Multi-provider Git repository manager

${BOLD}Usage:${RESET}
  repos-manager <provider> <command> [flags]
  repos-manager <command> [flags]

${BOLD}Providers:${RESET}
  github    GitHub (uses gh CLI)
  gitlab    GitLab (uses glab CLI)
  forgejo   Forgejo / Gitea (uses tea CLI)
  gitea     Alias for forgejo

${BOLD}Commands:${RESET}
  login [provider]   Authenticate (all detected providers if none specified)
  sync --all         Sync all providers
  status             Show dirty/ahead/behind repos across all providers

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

${BOLD}Filter file:${RESET}
  Create \$BASE_DIR/.repos-filter to sync ONLY matching repos:
    Dxsk/*
    my-org/specific-repo

${BOLD}Ignore file:${RESET}
  Create \$BASE_DIR/.repos-ignore to exclude repos:
    owner/repo-name
    org/*

${BOLD}Environment:${RESET}
  REPOS_MANAGER_BASE_DIR   Override default base directory

${BOLD}Examples:${RESET}
  repos-manager login
  repos-manager github login
  repos-manager github sync
  repos-manager github sync --filter Dxsk/*
  repos-manager sync --all --prune
  repos-manager status
  repos-manager gitlab sync --host gitlab.self-hosted.com
EOF
}

# ── Entry point ─────────────────────────────────────────────────────────────────

main() {
    check_deps
    validate_base_dir

    case "${1:-}" in
        github)
            shift
            case "${1:-}" in
                login) cmd_login "github" ;;
                sync)  shift; cmd_sync "github" "$@" ;;
                *)     echo "Usage: repos-manager github <login|sync>" >&2; exit 1 ;;
            esac
            ;;
        gitlab)
            shift
            case "${1:-}" in
                login) cmd_login "gitlab" ;;
                sync)  shift; cmd_sync "gitlab" "$@" ;;
                *)     echo "Usage: repos-manager gitlab <login|sync>" >&2; exit 1 ;;
            esac
            ;;
        forgejo|gitea)
            shift
            case "${1:-}" in
                login) cmd_login "forgejo" ;;
                sync)  shift; cmd_sync "forgejo" "$@" ;;
                *)     echo "Usage: repos-manager forgejo <login|sync>" >&2; exit 1 ;;
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
        version|--version|-v)
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
