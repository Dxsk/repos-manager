# sourceme.bash - Source this file to get the repos-manager function in bash
#
# Usage:
#   source /path/to/repos-manager/sourceme.bash
#   # or add to your .bashrc:
#   source ~/path/to/repos-manager/sourceme.bash

_REPOS_MANAGER_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

repos-manager() {
    bash "${_REPOS_MANAGER_ROOT}/repos-manager.sh" "$@"
}

# ── Completion ──────────────────────────────────────────────────────────────────

_repos_manager_complete() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    local prev="${COMP_WORDS[COMP_CWORD-1]}"

    case "$COMP_CWORD" in
        1)
            mapfile -t COMPREPLY < <(compgen -W "github gitlab forgejo gitea bitbucket radicle sync version help login status init update" -- "$cur")
            ;;
        2)
            case "$prev" in
                github|gitlab|forgejo|gitea|bitbucket|radicle)
                    mapfile -t COMPREPLY < <(compgen -W "login sync" -- "$cur")
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
