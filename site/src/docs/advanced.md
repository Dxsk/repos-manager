---
title: Advanced
description: "Advanced repos-manager topics: parallel jobs, per-host sync locks, self-update and package managers, fast status scans and Windows specifics."
order: 6
---

## Parallel sync

By default, repos-manager syncs 4 repos at a time. Adjust with:

```bash
repos-manager sync --all --parallel 8
```

Or set it in your config file:

{% raw %}
```json
{ "parallel": 8 }
```
{% endraw %}

For large organizations (100+ repos), higher parallelism helps. For slow connections, lower it to avoid timeouts.

## Concurrent sync protection

Each host has its own lock file, `<base_dir>/<host>/.repos-manager.lock`. Two syncs of the same host cannot run at the same time (the second one fails with `another sync is already running`), while syncs of different hosts can run in parallel since they write to separate directories.

The lock is an OS file lock held by the running process, not a PID check. If repos-manager is killed or crashes, the OS releases the lock immediately, so a leftover lock file never blocks the next run.

## Self-update

repos-manager can update itself:

```bash
repos-manager update        # asks for confirmation
repos-manager update --yes  # no prompt, for scripts
```

It queries the GitHub Releases API, downloads the asset built for the running platform, verifies it against the release `SHA256SUMS` and replaces the binary in place. A checksum mismatch aborts the update without touching the installed binary.

Installs owned by a package manager are left alone: when the binary lives under `/usr/bin` (AUR and other distro packages), a Homebrew prefix, a Scoop or winget folder or `~/.cargo/bin`, `update` exits with the package manager's command (`brew upgrade repos-manager`, `cargo install repos-manager`...) and the banner shows that command too.

Release assets are named after the Rust target triple:

| Platform | Asset |
|----------|-------|
| Linux x86_64 / aarch64 (static musl) | `repos-manager-x86_64-unknown-linux-musl.tar.gz`, `repos-manager-aarch64-unknown-linux-musl.tar.gz` |
| macOS Intel / Apple Silicon | `repos-manager-x86_64-apple-darwin.tar.gz`, `repos-manager-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 / ARM64 | `repos-manager-x86_64-pc-windows-msvc.zip`, `repos-manager-aarch64-pc-windows-msvc.zip` |
| Windows installers | `repos-manager-x86_64-setup.exe`, `repos-manager-aarch64-setup.exe` |
| Checksums | `SHA256SUMS` (archives and installers), `SHA256SUMS-binaries` (the binary inside each archive) |

The GitHub release carries all of them, and `update` always downloads from it (the `msvc` zip on Windows). The forge release only has the Linux musl archives, `repos-manager-x86_64-pc-windows-gnu.zip` and `repos-manager-x86_64-setup.exe`.

If repos-manager was installed system-wide (for example in `/usr/local/bin`), run `update` with the permissions needed to write there, or re-run the install script.

### Background update banner

You don't have to run `update` to know a new version is out. `login`, `sync` and `status` read a cached latest version from `<cache dir>/repos-manager/latest-version` and, when a newer release is available, print a one-line yellow banner on stderr before the command runs:

```bash
⬆ repos-manager 1.1.0 available (current 1.0.0), run: repos-manager update
```

When the cache is older than its TTL (24 hours by default), repos-manager spawns a detached copy of itself to refresh it from the GitHub Releases API. That child has its own process group, so it survives `Ctrl+C` on the parent, and it never delays the command in progress. Network errors are silent.

The check is disabled when:

- `REPOS_MANAGER_NO_UPDATE_CHECK=1` is set in the environment, or
- `check_updates: false` is set in `~/.config/repos-manager/config.json`.

Tune the cache location with `REPOS_MANAGER_UPDATE_CACHE` and the refresh interval with `REPOS_MANAGER_UPDATE_TTL` (seconds). `REPOS_MANAGER_UPDATE_URL` points both the check and `update` to another release API endpoint.

## Fast status on large workspaces

`repos-manager status` is built to stay responsive even when `base_dir` contains hundreds of repos and large dependency trees. Three things make it fast:

- **Heavy directory pruning.** The walk skips `node_modules`, `.venv`, `venv`, `__pycache__`, `target`, `vendor`, `dist`, `build`, `.next` and `.cache` before descending, and never descends into a `.git` directory, so vendored libraries never bloat the scan.
- **Network / FUSE mount skip (Linux).** `status` parses `/proc/self/mountinfo` and prunes every mount point under `base_dir` whose filesystem type is `fuse`, `fuse.*`, `nfs`, `cifs`, `smb*`, `smbfs`, `afs`, `ceph` or `davfs`. Cloud drives (kDrive, Dropbox, sshfs) are the number one cause of apparent hangs on `status`, and pruning them by default turns a multi-minute freeze into a sub-second scan. See the Configuration page, Network-mount scanning, to opt in if you really host repos on a reliable network share.
- **Streaming progress indicator.** Each repo is inspected as soon as the walk finds it, and a `[N] host/owner/repo` line is updated in place on stderr so you always see where the scan is. The indicator auto-disables when stderr is not a TTY or when `--quiet` is set.

Symlinks are not followed.

## Windows notes

- The config file stays at `%USERPROFILE%\.config\repos-manager\config.json`, the same layout as on Linux and macOS.
- The default base directory is `%USERPROFILE%\Documents`.
- A host with a port (`git.example.org:3000`) is stored in a directory named `git.example.org_3000`, because `:` is not allowed in Windows paths.
- tea's config is read from `%LOCALAPPDATA%\tea\config.yml` (see Providers).
- SSH cloning uses whatever `ssh` your `git` is configured with (Git for Windows ships its own). Use `--https` or `"protocol": "https"` to rely on Git Credential Manager instead.
- Shell completions for PowerShell: `repos-manager completions powershell | Out-String | Invoke-Expression`, added to your `$PROFILE`.

## Security

On Linux and macOS, the config directory (`~/.config/repos-manager/`) is created with mode `700` (owner-only access) and the Bitbucket API credentials file `bitbucket-creds` with mode `600`. On Windows these files inherit the ACLs of your user profile.

Self-updates and every install script verify release archives against `SHA256SUMS` before installing anything.
