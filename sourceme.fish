# sourceme.fish - Source this file to get the repos-manager function in fish
#
# Usage:
#   source /path/to/repos-manager/sourceme.fish
#   # or add to your ~/.config/fish/config.fish:
#   source ~/path/to/repos-manager/sourceme.fish

set -g _REPOS_MANAGER_ROOT (cd (dirname (status filename)); and pwd)

function repos-manager
    bash "$_REPOS_MANAGER_ROOT/repos-manager.sh" $argv
end

# ── Fish completion ─────────────────────────────────────────────────────────────

complete -c repos-manager -f

# Top-level commands
complete -c repos-manager -n "__fish_use_subcommand" -a "github"  -d "GitHub (gh)"
complete -c repos-manager -n "__fish_use_subcommand" -a "gitlab"  -d "GitLab (glab)"
complete -c repos-manager -n "__fish_use_subcommand" -a "forgejo" -d "Forgejo/Gitea (tea)"
complete -c repos-manager -n "__fish_use_subcommand" -a "gitea"   -d "Alias for forgejo"
complete -c repos-manager -n "__fish_use_subcommand" -a "bitbucket" -d "Bitbucket (bitbucket-cli or API)"
complete -c repos-manager -n "__fish_use_subcommand" -a "radicle" -d "Radicle (rad)"
complete -c repos-manager -n "__fish_use_subcommand" -a "sync"    -d "Sync repositories"
complete -c repos-manager -n "__fish_use_subcommand" -a "version" -d "Show version"
complete -c repos-manager -n "__fish_use_subcommand" -a "help"    -d "Show help"
complete -c repos-manager -n "__fish_use_subcommand" -a "login"   -d "Authenticate"
complete -c repos-manager -n "__fish_use_subcommand" -a "status"  -d "Show repo status"
complete -c repos-manager -n "__fish_use_subcommand" -a "init"    -d "Create default config"
complete -c repos-manager -n "__fish_use_subcommand" -a "update"  -d "Self-update"

# Provider subcommands
complete -c repos-manager -n "__fish_seen_subcommand_from github gitlab forgejo gitea bitbucket radicle" -a "login" -d "Authenticate with provider"
complete -c repos-manager -n "__fish_seen_subcommand_from github gitlab forgejo gitea bitbucket radicle" -a "sync"  -d "Sync repositories"

# Flags (after sync)
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l all      -d "Sync all providers"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l filter   -d "Filter repos by pattern" -r
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l base-dir -d "Base directory for repos" -r -F
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l https    -d "Use HTTPS instead of SSH"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l prune    -d "Remove local repos not on remote"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l dry-run  -d "Show what would be done without making changes"
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l host     -d "Custom host for self-hosted instances" -r
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l parallel -d "Number of parallel sync jobs" -r
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l verbose  -d "Show debug output" -s v
complete -c repos-manager -n "__fish_seen_subcommand_from sync" -l quiet    -d "Suppress info/success messages" -s q
