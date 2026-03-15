# sourceme.zsh - Source this file to get the repos-manager function in zsh
#
# Usage:
#   source /path/to/repos-manager/sourceme.zsh
#   # or add to your .zshrc:
#   source ~/path/to/repos-manager/sourceme.zsh

_REPOS_MANAGER_ROOT="$(cd "$(dirname "${(%):-%x}")" && pwd)"

repos-manager() {
    bash "${_REPOS_MANAGER_ROOT}/repos-manager.sh" "$@"
}

# ── Completion ──────────────────────────────────────────────────────────────────

_repos_manager_complete_zsh() {
    local -a providers=("github:GitHub (gh)" "gitlab:GitLab (glab)")
    local -a commands=("login:Authenticate with provider" "sync:Sync repositories")
    local -a global_cmds=("sync:Sync repositories" "version:Show version" "help:Show help")
    local -a flags=(
        "--filter:Filter repos by pattern"
        "--base-dir:Base directory for repos"
        "--https:Use HTTPS instead of SSH"
        "--prune:Remove local repos not on remote"
        "--dry-run:Show what would be done without making changes"
        "--host:Custom host for self-hosted instances"
    )

    case "$CURRENT" in
        2)
            _describe 'provider' providers
            _describe 'command' global_cmds
            ;;
        3)
            case "${words[2]}" in
                github|gitlab)
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
