---
title: Contributing
description: How to contribute to repos-manager.
order: 7
---

## Setup

```bash
git clone git@github.com:Dxsk/repos-manager.git
cd repos-manager
```

No build step needed - it's pure Bash.

## Running tests

```bash
# Install bats
sudo pacman -S bats    # Arch
brew install bats-core # macOS

# Run all tests
bats tests/*.bats
```

### Test coverage

The exact per-module counts drift as features land. Run `bats tests/*.bats` to see the current total and consult the bats files directly for the up-to-date list of cases. At time of writing the suite is ~90 tests covering CLI routing, flag parsing, pattern matching, config loading (including `check_updates` and `scan_network_mounts`), provider URL extraction and the Forgejo credentials helper, lockfile contention, `status` scanning (including the network-mount pruning and the vendored-directory prune), `sync_repo` paths and the background update-check module.

## Bash portability notes

repos-manager targets bash ≥ 4.0 at runtime, but the CI matrix runs the bats suite on both Ubuntu (bash 5) and macOS (bash 3.2, the system default). That asymmetry catches a handful of old-parser gotchas that have bitten the project. Keep them in mind when touching `lib/`:

- **`$(expr)` inside awk scripts breaks sourcing.** Bash 3.2 scans for `$(` greedily, even inside single-quoted strings, and an awk expression like `$(i+1)` inside a `awk '...'` here-script is mis-parsed as an unclosed shell command substitution. The visible symptom is `bad substitution: no closing ')' in <(` at the function declaration level, which makes every test using that file fail on macOS. **Fix:** hoist the field index into a plain awk variable first (`j = i + 1; $j`). See `lib/status.sh` inside `_status_network_mount_points` for a commented example.
- **Unset-array expansion.** `"${arr[@]}"` aborts under `set -u` when `arr` is empty on bash 3.2. Use `${arr[@]+"${arr[@]}"}` without outer quotes; the outer quoting wraps the empty expansion into a literal empty word, which on bash 3.2 passes an extra `""` argument to the following command.
- **`log_*` helpers and `set -e`.** A bare `return` after a predicate propagates the predicate's exit status, so `log_debug` returned 1 when `VERBOSE=false` and killed any caller under `set -e`. Always `return 0` explicitly when shortcutting out of a log helper.
- **`sort -z` defeats streaming.** `find ... | sort -z` forces `sort` to read every entry before emitting the first line, which silently turned the status progress indicator into a multi-minute hang on large trees. Rely on `find`'s natural order when streaming matters.
- **Colors on CI.** `log.sh` disables colors when stdout is not a TTY and when `NO_COLOR` is set. `tests/test_helper.bash` exports `NO_COLOR=1` so assertions like `[[ "$output" =~ "1 clean" ]]` do not have to deal with ANSI escapes.

## Linting

```bash
shellcheck -x repos-manager.sh lib/*.sh sourceme.bash
```

## CI pipeline

Every push triggers 4 checks:

| Check | Tool | What it does |
|-------|------|-------------|
| Bash lint | ShellCheck | Static analysis of all `.sh` files |
| Tests (Ubuntu) | Bats on bash 5 | Runs the full bats suite |
| Tests (macOS) | Bats on bash 3.2 | Same suite on Apple's legacy bash, catches old-parser regressions |
| Links | Lychee | Validates URLs in all markdown files |
| Shell compat | zsh + fish | Syntax checks on sourceme files |

All checks must pass before merging.

## Adding a provider

The provider system is modular. Each provider is a single file in `lib/` implementing 4 functions:

### Step 1 - Create the provider file

Create `lib/yourprovider.sh`:

```bash
#!/usr/bin/env bash
# YourProvider (uses yourcli)

yourprovider_login() {
    yourcli auth login
}

yourprovider_list_repos() {
    # Must return a JSON array of objects with:
    # nameWithOwner, sshUrl, url
    yourcli repo list --json ...
}

yourprovider_get_clone_url() {
    local repo_json="$1"
    if $USE_HTTPS; then
        echo "$repo_json" | jq -r '.url'
    else
        echo "$repo_json" | jq -r '.sshUrl'
    fi
}

yourprovider_get_full_name() {
    echo "$1" | jq -r '.nameWithOwner'
}
```

### Step 2 - Register it

In `repos-manager.sh`:

1. Source it: `source "${REPOS_MANAGER_LIB}/yourprovider.sh"`
2. Add to `VALID_PROVIDERS`
3. Add to `detect_providers()`
4. Add to `cmd_sync()` host mapping
5. Add to `cmd_sync_all()` providers array
6. Add case in `main()`

### Step 3 - Add tests

In `tests/providers.bats`:

```bash
@test "yourprovider: get ssh clone url" {
    USE_HTTPS=false
    local json='{"nameWithOwner":"user/repo","sshUrl":"git@host:user/repo.git","url":"https://host/user/repo"}'
    local result
    result=$(yourprovider_get_clone_url "$json")
    [[ "$result" == "git@host:user/repo.git" ]]
}
```

### Step 4 - Document

Update `readme.md`, `site/src/docs/providers.md`, and the glossary.

## Project structure

<pre style="background:var(--bg-secondary);border:1px solid var(--border);border-radius:8px;padding:1.2em;overflow-x:auto;font-size:0.85em;line-height:1.6"><code><span style="color:#3fb950">$</span> <span style="color:#58a6ff">tree</span> repos-manager/
<span style="color:#58a6ff">repos-manager/</span>
├── <span style="color:#3fb950">repos-manager.sh</span>        <span style="color:#8b949e"># Entry point, CLI routing</span>
├── <span style="color:#d29922">Makefile</span>                 <span style="color:#8b949e"># Install/uninstall</span>
├── <span style="color:#bc8cff">flake.nix</span>                <span style="color:#8b949e"># Nix flake (optional)</span>
├── <span style="color:#d29922">sourceme.bash</span>            <span style="color:#8b949e"># Bash shell integration</span>
├── <span style="color:#d29922">sourceme.zsh</span>             <span style="color:#8b949e"># Zsh shell integration</span>
├── <span style="color:#d29922">sourceme.fish</span>            <span style="color:#8b949e"># Fish shell integration</span>
├── <span style="color:#58a6ff">lib/</span>
│   ├── <span style="color:#3fb950">log.sh</span>               <span style="color:#8b949e"># Colors, log functions</span>
│   ├── <span style="color:#3fb950">flags.sh</span>             <span style="color:#8b949e"># Flag parsing</span>
│   ├── <span style="color:#3fb950">config.sh</span>            <span style="color:#8b949e"># Config file, sourceme gen</span>
│   ├── <span style="color:#3fb950">match.sh</span>             <span style="color:#8b949e"># Pattern matching, filter/ignore</span>
│   ├── <span style="color:#3fb950">sync.sh</span>              <span style="color:#8b949e"># Core sync engine (parallel)</span>
│   ├── <span style="color:#3fb950">status.sh</span>            <span style="color:#8b949e"># Status command (network-mount aware)</span>
│   ├── <span style="color:#3fb950">update.sh</span>            <span style="color:#8b949e"># Interactive self-update</span>
│   ├── <span style="color:#3fb950">update_check.sh</span>      <span style="color:#8b949e"># Background update check + banner</span>
│   ├── <span style="color:#3fb950">github.sh</span>            <span style="color:#8b949e"># GitHub provider</span>
│   ├── <span style="color:#3fb950">gitlab.sh</span>            <span style="color:#8b949e"># GitLab provider</span>
│   ├── <span style="color:#3fb950">forgejo.sh</span>           <span style="color:#8b949e"># Forgejo/Gitea provider</span>
│   ├── <span style="color:#3fb950">bitbucket.sh</span>         <span style="color:#8b949e"># Bitbucket provider</span>
│   └── <span style="color:#3fb950">radicle.sh</span>           <span style="color:#8b949e"># Radicle provider</span>
└── <span style="color:#58a6ff">tests/</span>
    ├── <span style="color:#d29922">test_helper.bash</span>     <span style="color:#8b949e"># Shared test utilities</span>
    ├── <span style="color:#d29922">cli.bats</span>             <span style="color:#8b949e"># CLI tests</span>
    ├── <span style="color:#d29922">flags.bats</span>           <span style="color:#8b949e"># Flag parsing tests</span>
    ├── <span style="color:#d29922">match.bats</span>           <span style="color:#8b949e"># Pattern matching tests</span>
    ├── <span style="color:#d29922">config.bats</span>          <span style="color:#8b949e"># Config tests</span>
    ├── <span style="color:#d29922">providers.bats</span>       <span style="color:#8b949e"># Provider URL + forgejo creds tests</span>
    ├── <span style="color:#d29922">status.bats</span>          <span style="color:#8b949e"># Status tests (incl. mount pruning)</span>
    ├── <span style="color:#d29922">sync.bats</span>            <span style="color:#8b949e"># Sync tests</span>
    ├── <span style="color:#d29922">lockfile.bats</span>        <span style="color:#8b949e"># Lockfile tests</span>
    └── <span style="color:#d29922">update_check.bats</span>    <span style="color:#8b949e"># Update-check banner tests</span></code></pre>

## Pull requests

| Rule | Details |
|------|---------|
| Branch from | `develop` |
| Scope | One feature per PR |
| Tests | Must pass (`bats tests/*.bats`) |
| Lint | Must pass (`shellcheck -x`) |
| Commits | Conventional style (`feat:`, `fix:`, `docs:`, `test:`) |
