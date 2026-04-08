#!/usr/bin/env bats

load test_helper

source "$REPOS_MANAGER_LIB/github.sh"
source "$REPOS_MANAGER_LIB/gitlab.sh"
source "$REPOS_MANAGER_LIB/forgejo.sh"
source "$REPOS_MANAGER_LIB/bitbucket.sh"
source "$REPOS_MANAGER_LIB/radicle.sh"

# ── Provider URL extraction ─────────────────────────────────────────────────────

@test "github: get ssh clone url" {
    USE_HTTPS=false
    local json='{"nameWithOwner":"Dxsk/repo","sshUrl":"git@github.com:Dxsk/repo.git","url":"https://github.com/Dxsk/repo"}'
    local result
    result=$(github_get_clone_url "$json")
    [[ "$result" == "git@github.com:Dxsk/repo.git" ]]
}

@test "github: get https clone url" {
    USE_HTTPS=true
    local json='{"nameWithOwner":"Dxsk/repo","sshUrl":"git@github.com:Dxsk/repo.git","url":"https://github.com/Dxsk/repo"}'
    local result
    result=$(github_get_clone_url "$json")
    [[ "$result" == "https://github.com/Dxsk/repo.git" ]]
}

@test "github: get full name" {
    local json='{"nameWithOwner":"Dxsk/repo","sshUrl":"git@github.com:Dxsk/repo.git","url":"https://github.com/Dxsk/repo"}'
    local result
    result=$(github_get_full_name "$json")
    [[ "$result" == "Dxsk/repo" ]]
}

@test "gitlab: get ssh clone url" {
    USE_HTTPS=false
    local json='{"nameWithOwner":"user/project","sshUrl":"git@gitlab.com:user/project.git","url":"https://gitlab.com/user/project.git"}'
    local result
    result=$(gitlab_get_clone_url "$json")
    [[ "$result" == "git@gitlab.com:user/project.git" ]]
}

@test "gitlab: get https clone url" {
    USE_HTTPS=true
    local json='{"nameWithOwner":"user/project","sshUrl":"git@gitlab.com:user/project.git","url":"https://gitlab.com/user/project.git"}'
    local result
    result=$(gitlab_get_clone_url "$json")
    [[ "$result" == "https://gitlab.com/user/project.git" ]]
}

@test "forgejo: get ssh clone url" {
    USE_HTTPS=false
    local json='{"nameWithOwner":"user/repo","sshUrl":"git@gitea.com:user/repo.git","url":"https://gitea.com/user/repo"}'
    local result
    result=$(forgejo_get_clone_url "$json")
    [[ "$result" == "git@gitea.com:user/repo.git" ]]
}

@test "forgejo: get https clone url" {
    USE_HTTPS=true
    local json='{"nameWithOwner":"user/repo","sshUrl":"git@gitea.com:user/repo.git","url":"https://gitea.com/user/repo"}'
    local result
    result=$(forgejo_get_clone_url "$json")
    [[ "$result" == "https://gitea.com/user/repo.git" ]]
}

@test "bitbucket: get ssh clone url" {
    USE_HTTPS=false
    local json='{"nameWithOwner":"user/repo","sshUrl":"git@bitbucket.org:user/repo.git","url":"https://bitbucket.org/user/repo.git"}'
    local result
    result=$(bitbucket_get_clone_url "$json")
    [[ "$result" == "git@bitbucket.org:user/repo.git" ]]
}

@test "forgejo: _forgejo_creds_for_host matches login by hostname" {
    export TEA_CONFIG="$TEST_TEMP/tea.yml"
    cat > "$TEA_CONFIG" <<'YAML'
logins:
    - name: example
      url: https://forge.example.org
      token: tok-example
    - name: other
      url: https://other.example.com/
      token: tok-other
YAML
    if ! command -v yq &>/dev/null; then
        skip "yq not installed"
    fi
    run _forgejo_creds_for_host "forge.example.org"
    [[ "$status" -eq 0 ]]
    [[ "$output" == "https://forge.example.org"$'\t'"tok-example" ]]
}

@test "forgejo: _forgejo_creds_for_host returns 2 when no login matches" {
    export TEA_CONFIG="$TEST_TEMP/tea.yml"
    cat > "$TEA_CONFIG" <<'YAML'
logins:
    - name: example
      url: https://forge.example.org
      token: tok-example
YAML
    if ! command -v yq &>/dev/null; then
        skip "yq not installed"
    fi
    run _forgejo_creds_for_host "codeberg.org"
    [[ "$status" -eq 2 ]]
}

@test "radicle: get clone url" {
    local json='{"nameWithOwner":"user/repo","sshUrl":"rad://z123abc","url":"rad://z123abc"}'
    local result
    result=$(radicle_get_clone_url "$json")
    [[ "$result" == "rad://z123abc" ]]
}
