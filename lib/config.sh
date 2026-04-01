#!/usr/bin/env bash
# Configuration file support
# Reads ~/.config/repos-manager/config.json

REPOS_MANAGER_CONFIG="${REPOS_MANAGER_CONFIG:-$HOME/.config/repos-manager/config.json}"

load_config() {
    [[ ! -f "$REPOS_MANAGER_CONFIG" ]] && return

    local val

    # Base directory
    val=$(jq -r '.base_dir // empty' "$REPOS_MANAGER_CONFIG" 2>/dev/null)
    [[ -n "$val" ]] && BASE_DIR="${val/#\~/$HOME}"

    # Default parallel jobs
    val=$(jq -r '.parallel // empty' "$REPOS_MANAGER_CONFIG" 2>/dev/null)
    [[ -n "$val" ]] && PARALLEL="$val"

    # Default protocol
    val=$(jq -r '.protocol // empty' "$REPOS_MANAGER_CONFIG" 2>/dev/null)
    [[ "$val" == "https" ]] && USE_HTTPS=true

    # Custom hosts
    val=$(jq -r '.hosts.gitlab // empty' "$REPOS_MANAGER_CONFIG" 2>/dev/null)
    [[ -n "$val" && -z "$HOST" ]] && HOST="$val"
}

init_config() {
    local config_dir
    config_dir=$(dirname "$REPOS_MANAGER_CONFIG")
    mkdir -p "$config_dir"

    if [[ -f "$REPOS_MANAGER_CONFIG" ]]; then
        log_warn "Config already exists: $REPOS_MANAGER_CONFIG"
        return
    fi

    cat > "$REPOS_MANAGER_CONFIG" <<'JSON'
{
  "base_dir": "~/Documents",
  "parallel": 4,
  "protocol": "ssh",
  "hosts": {
    "gitlab": "gitlab.com",
    "forgejo": "gitea.com"
  }
}
JSON

    log_success "Config created: $REPOS_MANAGER_CONFIG"
}

generate_sourceme() {
    local host_dir="$1"
    local host_name
    host_name=$(basename "$host_dir")

    cat > "$host_dir/sourceme" <<SOURCEME
#!/bin/bash

repos-manager() {
    bash "\$HOME/.local/bin/repos-manager" "\$@"
}
SOURCEME

    cat > "$host_dir/sourceme.zsh" <<SOURCEME
_REPOS_MANAGER_ROOT="\$(cd "\$(dirname "\${(%):-%x}")" && pwd)"

repos-manager() {
    bash "\$HOME/.local/bin/repos-manager" "\$@"
}
SOURCEME

    cat > "$host_dir/sourceme.fish" <<SOURCEME
function repos-manager
    bash "\$HOME/.local/bin/repos-manager" \$argv
end
SOURCEME
}
