#!/usr/bin/env bash
set -euo pipefail

VERSION="0.1.0"
BASE_DIR="${REPOS_MANAGER_BASE_DIR:-$HOME/Documents}"

# Resolve lib directory (overridable for Nix packaging)
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
# shellcheck source=lib/sync.sh
source "${REPOS_MANAGER_LIB}/sync.sh"

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

# ── Commands ────────────────────────────────────────────────────────────────────

readonly VALID_PROVIDERS="github gitlab"

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
    local provider="$1"
    validate_provider "$provider"
    "${provider}_login"
}

cmd_sync() {
    local provider="$1"
    validate_provider "$provider"
    shift
    parse_flags "$@"

    local host
    case "$provider" in
        github) host="github.com" ;;
        gitlab) host="${HOST:-gitlab.com}" ;;
        *) echo "Unknown provider: $provider" >&2; exit 1 ;;
    esac

    sync_provider "$provider" "$host"
}

cmd_sync_all() {
    parse_flags "$@"

    local -a providers=("github:github.com" "gitlab:${HOST:-gitlab.com}")

    for entry in "${providers[@]}"; do
        local provider="${entry%%:*}"
        local host="${entry##*:}"

        local cli
        case "$provider" in
            github) cli="gh" ;;
            gitlab) cli="glab" ;;
        esac

        if ! command -v "$cli" &>/dev/null; then
            log_warn "Skipping ${provider}: ${cli} not found"
            continue
        fi

        printf "\n${BOLD}=== Syncing %s ===${RESET}\n\n" "$provider"
        sync_provider "$provider" "$host" || true
    done
}

# ── Usage ───────────────────────────────────────────────────────────────────────

print_usage() {
    cat <<EOF
${BOLD}repos-manager${RESET} - Multi-provider Git repository manager

${BOLD}Usage:${RESET}
  repos-manager <provider> <command> [flags]
  repos-manager sync --all [flags]

${BOLD}Providers:${RESET}
  github    GitHub (uses gh CLI)
  gitlab    GitLab (uses glab CLI)

${BOLD}Commands:${RESET}
  login     Authenticate with the provider
  sync      Sync repositories

${BOLD}Flags:${RESET}
  --filter <pattern>   Filter repos by pattern (e.g., Dxsk/* or Dxsk/project)
  --base-dir <path>    Base directory (default: ~/Documents)
  --https              Use HTTPS instead of SSH
  --prune              Remove local repos not on remote
  --dry-run            Show what would be done without making changes
  --host <host>        Custom host (for self-hosted GitLab)

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
  repos-manager github sync
  repos-manager github sync --filter Dxsk/*
  repos-manager github sync --filter YouUser/*
  repos-manager github sync --filter Dxsk/git-chronicles
  repos-manager github sync --filter YouUser/YouRepos
  repos-manager gitlab sync --host gitlab.self-hosted.com
  repos-manager sync --all --prune
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
        sync)
            shift
            if [[ "${1:-}" != "--all" ]]; then
                echo "Usage: repos-manager sync --all [flags]" >&2
                exit 1
            fi
            cmd_sync_all "$@"
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
