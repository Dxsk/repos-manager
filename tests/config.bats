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

@test "init_config: does not overwrite existing config" {
    REPOS_MANAGER_CONFIG="$TEST_TEMP/existing.json"
    echo '{"custom": true}' > "$REPOS_MANAGER_CONFIG"
    init_config
    jq -e '.custom' "$REPOS_MANAGER_CONFIG"
}
