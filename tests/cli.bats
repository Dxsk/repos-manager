#!/usr/bin/env bats

SCRIPT="$(cd "$(dirname "$BATS_TEST_FILENAME")/.." && pwd)/repos-manager.sh"

@test "cli: --version shows version" {
    run bash "$SCRIPT" --version
    [[ "$status" -eq 0 ]]
    [[ "$output" =~ "repos-manager 0.3.0" ]]
}

@test "cli: --help shows usage" {
    run bash "$SCRIPT" --help
    [[ "$status" -eq 0 ]]
    [[ "$output" =~ "Multi-provider Git repository manager" ]]
}

@test "cli: no args shows usage" {
    run bash "$SCRIPT"
    [[ "$status" -eq 0 ]]
    [[ "$output" =~ "Usage:" ]]
}

@test "cli: unknown command fails" {
    run bash "$SCRIPT" nonexistent
    [[ "$status" -ne 0 ]]
    [[ "$output" =~ "Unknown command" ]]
}

@test "cli: github without subcommand fails" {
    run bash "$SCRIPT" github
    [[ "$status" -ne 0 ]]
    [[ "$output" =~ "Usage:" ]]
}

@test "cli: sync without --all fails" {
    run bash "$SCRIPT" sync
    [[ "$status" -ne 0 ]]
}

@test "cli: help shows all providers" {
    run bash "$SCRIPT" --help
    [[ "$output" =~ "github" ]]
    [[ "$output" =~ "gitlab" ]]
    [[ "$output" =~ "forgejo" ]]
    [[ "$output" =~ "bitbucket" ]]
    [[ "$output" =~ "radicle" ]]
}

@test "cli: help shows all commands" {
    run bash "$SCRIPT" --help
    [[ "$output" =~ "login" ]]
    [[ "$output" =~ "sync" ]]
    [[ "$output" =~ "status" ]]
    [[ "$output" =~ "init" ]]
    [[ "$output" =~ "update" ]]
}
