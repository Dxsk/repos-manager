#!/usr/bin/env bats

load test_helper

@test "parse_flags: default values" {
    parse_flags
    [[ "$FILTER" == "" ]]
    [[ "$USE_HTTPS" == "false" ]]
    [[ "$PRUNE" == "false" ]]
    [[ "$DRY_RUN" == "false" ]]
    [[ "$HOST" == "" ]]
    [[ "$PARALLEL" == "4" ]]
}

@test "parse_flags: --filter" {
    parse_flags --filter "Dxsk/*"
    [[ "$FILTER" == "Dxsk/*" ]]
}

@test "parse_flags: --filter=value" {
    parse_flags --filter=Dxsk/repo
    [[ "$FILTER" == "Dxsk/repo" ]]
}

@test "parse_flags: --https" {
    parse_flags --https
    [[ "$USE_HTTPS" == "true" ]]
}

@test "parse_flags: --prune" {
    parse_flags --prune
    [[ "$PRUNE" == "true" ]]
}

@test "parse_flags: --dry-run" {
    parse_flags --dry-run
    [[ "$DRY_RUN" == "true" ]]
}

@test "parse_flags: --host" {
    parse_flags --host gitlab.example.com
    [[ "$HOST" == "gitlab.example.com" ]]
}

@test "parse_flags: --parallel" {
    parse_flags --parallel 8
    [[ "$PARALLEL" == "8" ]]
}

@test "parse_flags: multiple flags" {
    parse_flags --filter "org/*" --https --prune --dry-run --parallel 16
    [[ "$FILTER" == "org/*" ]]
    [[ "$USE_HTTPS" == "true" ]]
    [[ "$PRUNE" == "true" ]]
    [[ "$DRY_RUN" == "true" ]]
    [[ "$PARALLEL" == "16" ]]
}

@test "parse_flags: unknown flag exits" {
    run parse_flags --unknown
    [[ "$status" -ne 0 ]]
}

@test "parse_flags: --all is silently consumed" {
    parse_flags --all
    # Should not error
    [[ "$?" -eq 0 ]]
}
