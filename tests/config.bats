#!/usr/bin/env bats

load test_helper

@test "load_config: no config file does nothing" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/nonexistent.json"
    BASE_DIR="/original"
    load_config
    [[ "$BASE_DIR" == "/original" ]]
}

@test "load_config: reads base_dir" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/config.json"
    cat > "$REPOS_MANAGER_CONFIG" <<'JSON'
{"base_dir": "/custom/path"}
JSON
    load_config
    [[ "$BASE_DIR" == "/custom/path" ]]
}

@test "load_config: expands tilde in base_dir" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/config.json"
    cat > "$REPOS_MANAGER_CONFIG" <<'JSON'
{"base_dir": "~/repos"}
JSON
    load_config
    [[ "$BASE_DIR" == "$HOME/repos" ]]
}

@test "load_config: reads parallel" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/config.json"
    cat > "$REPOS_MANAGER_CONFIG" <<'JSON'
{"parallel": 16}
JSON
    PARALLEL=4
    load_config
    [[ "$PARALLEL" == "16" ]]
}

@test "load_config: reads protocol https" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/config.json"
    cat > "$REPOS_MANAGER_CONFIG" <<'JSON'
{"protocol": "https"}
JSON
    USE_HTTPS=false
    load_config
    [[ "$USE_HTTPS" == "true" ]]
}

@test "load_config: ssh protocol keeps default" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/config.json"
    cat > "$REPOS_MANAGER_CONFIG" <<'JSON'
{"protocol": "ssh"}
JSON
    USE_HTTPS=false
    load_config
    [[ "$USE_HTTPS" == "false" ]]
}

@test "init_config: creates config file" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/new_config.json"
    init_config
    [[ -f "$REPOS_MANAGER_CONFIG" ]]
    jq -e '.base_dir' "$REPOS_MANAGER_CONFIG"
    jq -e '.parallel' "$REPOS_MANAGER_CONFIG"
}

@test "env var BASE_DIR overrides config file" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/override.json"
    mkdir -p "$TEST_TEMP/env_dir"
    cat > "$REPOS_MANAGER_CONFIG" <<JSON
{
  "base_dir": "/tmp/from-config"
}
JSON

    export REPOS_MANAGER_BASE_DIR="$TEST_TEMP/env_dir"
    BASE_DIR="$REPOS_MANAGER_BASE_DIR"
    load_config
    [[ "$BASE_DIR" == "$TEST_TEMP/env_dir" ]]
}

@test "load_config: parses hosts as array" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/multi.json"
    cat > "$REPOS_MANAGER_CONFIG" <<'JSON'
{
  "hosts": {
    "gitlab":  ["gitlab.com", "gitlab.example.org"],
    "forgejo": ["codeberg.org", "forge.example.org"]
  }
}
JSON
    load_config
    [[ "${#HOSTS_GITLAB[@]}" -eq 2 ]]
    [[ "${HOSTS_GITLAB[0]}" == "gitlab.com" ]]
    [[ "${HOSTS_GITLAB[1]}" == "gitlab.example.org" ]]
    [[ "${#HOSTS_FORGEJO[@]}" -eq 2 ]]
    [[ "${HOSTS_FORGEJO[1]}" == "forge.example.org" ]]
}

@test "load_config: accepts legacy string host form" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/legacy.json"
    cat > "$REPOS_MANAGER_CONFIG" <<'JSON'
{"hosts": {"gitlab": "gitlab.example.com"}}
JSON
    load_config
    [[ "${#HOSTS_GITLAB[@]}" -eq 1 ]]
    [[ "${HOSTS_GITLAB[0]}" == "gitlab.example.com" ]]
}

@test "provider_hosts: returns configured hosts" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/ph.json"
    cat > "$REPOS_MANAGER_CONFIG" <<'JSON'
{"hosts": {"forgejo": ["a.example", "b.example"]}}
JSON
    load_config
    run provider_hosts forgejo
    [[ "$status" -eq 0 ]]
    [[ "$output" == *"a.example"* ]]
    [[ "$output" == *"b.example"* ]]
}

@test "init_config: does not overwrite existing config" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/existing.json"
    echo '{"custom": true}' > "$REPOS_MANAGER_CONFIG"
    init_config
    jq -e '.custom' "$REPOS_MANAGER_CONFIG"
}
