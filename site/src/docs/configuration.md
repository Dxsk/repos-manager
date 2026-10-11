---
title: Configuration
description: "Configure repos-manager with config.json and environment variables: base directory, protocol, parallel jobs, several hosts per provider and plain git servers."
order: 2
---

## Config file

Generate a default config:

```bash
repos-manager init
```

This creates `~/.config/repos-manager/config.json`. The path is the same on every OS, including Windows (`%USERPROFILE%\.config\repos-manager\config.json`), and can be overridden with `REPOS_MANAGER_CONFIG`.

{% raw %}
```json
{
  "base_dir": "~/Documents",
  "parallel": 4,
  "protocol": "ssh",
  "check_updates": true,
  "scan_network_mounts": false,
  "gitlab_statistics": true,
  "hosts": {
    "github":    ["github.com"],
    "gitlab":    ["gitlab.com"],
    "forgejo":   ["codeberg.org"],
    "bitbucket": ["bitbucket.org"]
  },
  "repos": {}
}
```
{% endraw %}

| Key | Description | Default |
|-----|-------------|---------|
| `base_dir` | Root directory for all repos (`~` is expanded, must resolve to an absolute path) | `~/Documents` |
| `parallel` | Number of parallel sync jobs | `4` |
| `protocol` | Clone protocol (`ssh` or `https`) | `ssh` |
| `check_updates` | Check for a newer release and show a banner | `true` |
| `scan_network_mounts` | Let `status` traverse FUSE, NFS or SMB mounts under `base_dir` (see warning below) | `false` |
| `gitlab_statistics` | Ask GitLab for repository sizes. The full project serializer makes the listing heavier than `simple=true`; set `false` to turn it off, sizes then stay unknown | `true` |
| `hosts.<provider>` | List of hosts to sync for that provider | See defaults below |
| `repos.<host>` | Plain git servers with no forge API, synced by the `any` provider | Empty |

An invalid JSON file is reported as an error instead of being silently ignored.

### Update banner

When `check_updates` is enabled, `login`, `sync` and `status` read a cached "latest version" and print a single-line yellow banner when it is newer than the running binary. If the cache is older than 24 hours, a detached background copy of repos-manager refreshes it from the GitHub Releases API. The check never blocks the command in progress and stays silent when nothing new is available or the network is down.

Opt out at any time without editing the config file:

```bash
REPOS_MANAGER_NO_UPDATE_CHECK=1 repos-manager status
```

### Network-mount scanning

`repos-manager status` walks `base_dir` recursively to collect every `.git` directory. Cloud drives mounted via FUSE (kDrive, Dropbox, sshfs, and similar) and network filesystems (NFS, SMB/CIFS, davfs, Ceph, AFS) turn each readdir into a blocking round-trip, and a single repo on such a mount can hang the scan for minutes.

On Linux, `status` reads `/proc/self/mountinfo` by default, finds every mount point under `base_dir` whose filesystem type matches `fuse`, `fuse.*`, `nfs`, `cifs`, `smb*`, `smbfs`, `afs`, `ceph` or `davfs`, and prunes it from the walk. Use `--verbose` to see which mounts were skipped. On macOS and Windows there is no such detection.

Set `scan_network_mounts: true` only if you really do host working copies on a reliable network share. The scan will then descend into those mounts and print a warning that this can cause long hangs on flaky links.

### GitLab repository sizes

GitLab only reports a repository size when the listing uses its full project serializer (`statistics=true`), which makes the payload heavier than the `simple=true` listing used before. That cost has not been measured against a real instance. On a very large GitLab, set `"gitlab_statistics": false` to go back to the light listing: every GitLab repository then has an unknown size. Projects where you are below the Reporter role have an unknown size either way.

### Multiple hosts per provider

Each provider key under `hosts` accepts an **array** of hostnames, so you can sync several GitHub, GitLab or Forgejo instances side-by-side (typical setup: a SaaS one + your self-hosted one).

{% raw %}
```json
{
  "hosts": {
    "gitlab":  ["gitlab.com", "gitlab.example.org"],
    "forgejo": ["codeberg.org", "forge.example.org"]
  }
}
```
{% endraw %}

`repos-manager gitlab sync` and `repos-manager sync --all` will then iterate over every configured host. Repositories are mirrored under `<base_dir>/<host>/...`, so each instance gets its own directory tree and its own sync lock.

The legacy string form (`"gitlab": "gitlab.com"`) is still accepted and treated as a single-element list. `gitea` and `gogs` are accepted as aliases of `forgejo`, and `srht` as an alias of `sourcehut`.

**Per-CLI requirements for self-hosted instances:**

- **GitHub Enterprise**: `repos-manager` passes `--hostname <host>` to `gh`. Run `gh auth login --hostname <host>` once per host.
- **GitLab**: `repos-manager` sets `GITLAB_HOST` so `glab` targets the right instance. Run `glab auth login --hostname <host>` once per host.
- **Forgejo / Gitea / Gogs**: `repos-manager` reads the matching login entry (URL + token) from tea's config file and talks to the Forgejo REST API directly. Run `tea login add` once per instance, or `repos-manager forgejo login` to save a base URL and a token in `forgejo-creds` when `tea` is missing or refuses the server, which is how Gogs is reached. A `tea` login wins over a saved token; when neither matches, the host is **skipped with a warning** and the sync continues with the next one.
- **Bitbucket**: `bitbucket.org` goes through the Cloud 2.0 API with an Atlassian API token; **any other host is treated as Data Center / Server** and goes through the 1.0 API with an HTTP access token. Run `repos-manager bitbucket login` once per host.
- **Sourcehut**: `repos-manager` passes `--instance <host>` to `hut`. Run `hut init` once per instance.
- **OneDev**: credentials are stored per host by `repos-manager onedev login`. OneDev is self-hosted only, so it has no default host.
- **Radicle**: the host is only used for the directory name; listing always targets your local Radicle node.
- **Plain git servers (`any`)**: hosts come from the `repos` key, not from `hosts`, and are reached with your own SSH keys. See below.

You can still override the configured list for a one-off sync with `repos-manager <provider> sync --host <hostname>`.

### Defaults

If `hosts` is missing or empty for a provider, `repos-manager` falls back to its default host, which is the SaaS one where there is one:

| Provider | Default host |
|----------|--------------|
| `github` | `github.com` |
| `gitlab` | `gitlab.com` |
| `forgejo` | `codeberg.org` |
| `bitbucket` | `bitbucket.org` |
| `radicle` | `radicle` (local node) |
| `sourcehut` | `git.sr.ht` |
| `onedev` | None, self-hosted only |
| `any` | None, the keys of `repos` |

`onedev` has no default: nothing is synced until you add `"hosts": {"onedev": ["onedev.example.org"]}`. The `any` provider takes its hosts from the keys of the `repos` key described below, so it needs no `hosts` entry either.

### Plain git servers (`repos`)

A bare git repository served over SSH has no forge API to ask, so the `any` provider reads the top-level `repos` key instead. Its keys are hosts, and each key doubles as the directory name under `base_dir`.

An entry is either a list of clone URLs, or an object with `urls`, `discover`, `user` and `port`. Both forms are accepted.

**Clone URLs listed by hand**

{% raw %}
```json
{
  "repos": {
    "git.example.org": [
      "git@git.example.org:infra/ansible.git",
      "ssh://me@vps.example.org:2222/srv/git/notes.git",
      "https://git.example.org/a/b/c.git"
    ]
  }
}
```
{% endraw %}

The local path comes from the URL: the three above land in `infra/ansible`, `srv/git/notes` and `a/b/c` under their host directory. A listed URL is used exactly as written, so `--https` does not rewrite it.

**gitolite**

{% raw %}
```json
{ "repos": { "gitolite.example.org": { "discover": "gitolite", "user": "git" } } }
```
{% endraw %}

Runs `ssh [-p port] <user>@<host> info` and reads the repository list out of it. `user` defaults to `git`, `port` is optional. Wildcard entries of the gitolite config (patterns, `CREATOR`) are not repositories and are dropped.

**Soft Serve**

{% raw %}
```json
{ "repos": { "soft.example.org": { "discover": "soft-serve", "port": 23231 } } }
```
{% endraw %}

Runs `ssh -p <port> <host> repo list`. Port defaults to 23231, [Soft Serve](https://github.com/charmbracelet/soft-serve)'s default.

**A directory of bare repositories**

{% raw %}
```json
{ "repos": { "vps.example.org": { "discover": "find:/srv/git", "user": "me", "port": 2222 } } }
```
{% endraw %}

Runs `find /srv/git -name '*.git' -prune` over SSH. This one needs a real shell on the remote account: it does not work with `git-shell` unless a `list` script sits in `~/git-shell-commands`.

**Both at once**

`urls` and `discover` can be combined in one entry, and the union is synced:

{% raw %}
```json
{
  "repos": {
    "vps.example.org": {
      "urls": ["ssh://me@vps.example.org:2222/srv/git/legacy.git"],
      "discover": "find:/srv/git",
      "user": "me",
      "port": 2222
    }
  }
}
```
{% endraw %}

Every discovery command runs with `BatchMode=yes` and a connect timeout, so an unknown host key or a passphrase with no agent fails with a message instead of blocking the sync. There is nothing to log in to: these hosts use your own SSH keys.

### Credential files

Three providers store their own credentials next to the config file, under `~/.config/repos-manager/`. Each one is a JSON map keyed by host, created with mode `600` on Linux and macOS, so several instances of the same provider coexist.

| File | Written by | Holds |
|------|------------|-------|
| `bitbucket-creds` | `repos-manager bitbucket login` | Atlassian API token (Cloud, with the account email) or HTTP access token (Data Center) |
| `forgejo-creds` | `repos-manager forgejo login` | Base URL and access token, used when `tea` is missing or declined |
| `onedev-creds` | `repos-manager onedev login` | Account name and access token per OneDev instance |

GitHub, GitLab, Forgejo through `tea`, Sourcehut and Radicle keep their credentials in their own CLI's configuration, and the `any` provider has none. See the Providers page for the details of each file.

## Environment variables

| Variable | Description | Default |
|----------|-------------|---------|
| `REPOS_MANAGER_CONFIG` | Path to config file | `~/.config/repos-manager/config.json` |
| `REPOS_MANAGER_BASE_DIR` | Override base directory | `~/Documents` |
| `REPOS_MANAGER_PARALLEL` | Override parallel jobs | `4` |
| `REPOS_MANAGER_PROTOCOL` | Override protocol (`ssh` or `https`) | `ssh` |
| `REPOS_MANAGER_NO_UPDATE_CHECK` | Set to `1` to skip the background update check | Unset |
| `REPOS_MANAGER_UPDATE_TTL` | Seconds between update checks (cache TTL) | `86400` |
| `REPOS_MANAGER_UPDATE_CACHE` | Path to the cached latest-version file | `<cache dir>/repos-manager/latest-version` |
| `REPOS_MANAGER_GIT_TIMEOUT` | Seconds before a single git command is killed | `3600` |
| `REPOS_MANAGER_LIST_TIMEOUT` | Seconds before a provider CLI listing is killed | `300` |
| `REPOS_MANAGER_UPDATE_URL` | Release API endpoint used by the update check and `update` | GitHub Releases API |
| `TEA_CONFIG` | Path to tea's config file, read by the Forgejo provider | tea's default location (see Providers) |
| `NO_COLOR` | Disable colored output | Unset |

The cache directory is `$XDG_CACHE_HOME` when set, otherwise the OS cache directory (`~/.cache` on Linux, `~/Library/Caches` on macOS, `%LOCALAPPDATA%` on Windows).

Environment variables take precedence over the config file. Flags take precedence over both.
