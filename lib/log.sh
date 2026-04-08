#!/usr/bin/env bash
# Logging and color utilities
# shellcheck disable=SC2034  # Variables used in other sourced files

# Respect NO_COLOR (https://no-color.org/) and detect non-TTY output
if [[ -z "${NO_COLOR:-}" ]] && [[ -t 1 ]]; then
    RED=$'\033[31m'
    GREEN=$'\033[32m'
    YELLOW=$'\033[33m'
    BLUE=$'\033[34m'
    GRAY=$'\033[90m'
    BOLD=$'\033[1m'
    RESET=$'\033[0m'
else
    RED="" GREEN="" YELLOW="" BLUE="" GRAY="" BOLD="" RESET=""
fi

# Note: each predicate uses `return 0` rather than a bare `return` so the
# function exits with status 0 when logging is disabled. A bare `return`
# propagates the status of the preceding test, which under `set -e`
# aborts any caller that invokes the helper outside a conditional.
log_info()    { $QUIET && return 0; printf "%s%s%s\n" "$BLUE" "$*" "$RESET"; }
log_success() { $QUIET && return 0; printf "  %s✓ %s%s\n" "$GREEN" "$*" "$RESET"; }
log_warn()    { printf "  %s⚠ %s%s\n" "$YELLOW" "$*" "$RESET"; }
log_error()   { printf "  %s✗ %s%s\n" "$RED" "$*" "$RESET"; }
log_skip()    { $QUIET && return 0; printf "  %s⊘ %s%s\n" "$GRAY" "$*" "$RESET"; }
log_debug()   { $VERBOSE || return 0; printf "  %s[debug] %s%s\n" "$GRAY" "$*" "$RESET"; }
