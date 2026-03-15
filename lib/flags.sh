#!/usr/bin/env bash
# Global flags and argument parsing
# These variables are used by other sourced modules (sync.sh, github.sh, etc.)

# shellcheck disable=SC2034

FILTER=""
USE_HTTPS=false
PRUNE=false
DRY_RUN=false
HOST=""

parse_flags() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --filter)     FILTER="$2"; shift 2 ;;
            --filter=*)   FILTER="${1#--filter=}"; shift ;;
            --base-dir)   BASE_DIR="$2"; shift 2 ;;
            --base-dir=*) BASE_DIR="${1#--base-dir=}"; shift ;;
            --https)      USE_HTTPS=true; shift ;;
            --prune)      PRUNE=true; shift ;;
            --dry-run)    DRY_RUN=true; shift ;;
            --host)       HOST="$2"; shift 2 ;;
            --host=*)     HOST="${1#--host=}"; shift ;;
            --all)        shift ;; # consumed by caller
            *) echo "Unknown flag: $1" >&2; exit 1 ;;
        esac
    done
}
