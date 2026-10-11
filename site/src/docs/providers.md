---
title: Providers
description: "Set up GitHub, GitLab, Forgejo/Gitea/Gogs, Bitbucket, Radicle, Sourcehut, OneDev and plain git servers with repos-manager, including self-hosted instances and collaborator repos."
order: 3
---

## Provider status

| Provider | Built from | Exercised against |
|----------|------------|-------------------|
| GitHub | `gh` and its API output | Used day to day; listing covered by offline tests |
| GitLab | `glab` and its API output | Used day to day; listing covered by offline tests |
| Forgejo / Gitea / Gogs | `tea` and the Gitea API | Used day to day; listing covered by offline tests |
| Bitbucket Cloud | The Bitbucket Cloud REST API | Offline tests, plus a local HTTP server for paging |
| Bitbucket Data Center | The published Data Center REST API | Offline tests and a local HTTP server only. Never run against a live instance |
| Radicle | `rad` | Offline tests |
| Sourcehut | `hut` and the published GraphQL API | Offline tests of payload parsing and command construction only. Never run against a live instance |
| OneDev | The published REST API | Offline tests and a local HTTP server only. Never run against a live instance |
| Plain git servers (`any`), config URLs | The `repos` config | Offline tests |
| Plain git servers (`any`), SSH discovery: `gitolite`, `soft-serve`, `find:<root>` | The documented output of each tool | Offline tests of the output parsing and command construction only. Never run against a live server |

### Best effort surfaces

Sourcehut, OneDev, Bitbucket Data Center and the three SSH discovery modes of the `any` provider (`gitolite`, `soft-serve`, `find:<root>`) are best effort. They are implemented against the published API documentation and covered by offline tests only. No live instance of any of them has ever answered a request from `repos-manager`.

GitHub, GitLab and Forgejo are the paths the project is used through day to day. Their listing is covered by offline tests in the same way, which is not a guarantee against every server version or configuration.

If one of these surfaces fails or lists nothing, please [open an issue](https://github.com/Dxsk/repos-manager/issues) with:

- the raw payload (or command output) the server returned, with tokens and private names removed
- the server version

Without access to such a server, that payload is what makes a fix possible, and it becomes a test fixture.

Most providers are driven through their official CLI, which must be in your `PATH`. OneDev has no CLI and authenticates with a token saved by `repos-manager onedev login`, and the `any` provider needs nothing but `git` and `ssh`. `repos-manager login` without argument runs the login flow of every provider it can use.

---

## GitHub

**CLI:** [`gh`](https://cli.github.com/) ([docs](https://cli.github.com/manual/))

```bash
# Install
sudo pacman -S github-cli      # Arch
brew install gh                # macOS
winget install GitHub.cli      # Windows

# Authenticate
repos-manager github login

# Sync
repos-manager github sync
```

Listing calls `gh api --paginate "/user/repos?affiliation=owner,collaborator,organization_member"`, so it covers your own repos, the repos of organizations you belong to, and repos on other personal accounts where you are a collaborator.

For GitHub Enterprise, run `gh auth login --hostname <host>` and add the host to `hosts.github` (or pass `--host`).

## GitLab

**CLI:** [`glab`](https://gitlab.com/gitlab-org/cli) ([docs](https://gitlab.com/gitlab-org/cli/-/blob/main/README.md))

```bash
# Install
sudo pacman -S glab        # Arch
brew install glab          # macOS
winget install GLab.GLab   # Windows

# Self-hosted
glab auth login --hostname gitlab.company.com
repos-manager gitlab sync --host gitlab.company.com
```

Lists every project you are a member of (`projects?membership=true`), with pagination for large instances. Groups and subgroups are mirrored as nested directories. Works with GitLab.com and self-hosted.

## Forgejo / Gitea / Gogs

**CLI:** [`tea`](https://gitea.com/gitea/tea) ([Forgejo docs](https://forgejo.org/docs/latest/)), or a token saved by `repos-manager forgejo login`

`gitea` and `gogs` are accepted as aliases: `repos-manager gitea sync` and `repos-manager gogs sync` are the same as `repos-manager forgejo sync`.

```bash
# Install tea for authentication
sudo pacman -S tea   # Arch
brew install tea     # macOS

# Authenticate once per instance
tea login add
# or
repos-manager forgejo login

# Self-hosted
repos-manager forgejo sync --host git.example.com
```

### How listing works

`tea repo list` only returns the authenticated user's own repositories and does not enumerate organizations, so `repos-manager` talks to the Forgejo REST API directly instead. It reads the per-host URL and token from tea's config file, then paginates three endpoints and merges the results:

- `/api/v1/user/repos` for your personal repos and collaborations
- `/api/v1/user/orgs` for the organizations you belong to
- `/api/v1/orgs/{org}/repos` for every such organization

`tea` is only used to create the login entry; it is not called during sync. The config file is the source of truth for credentials.

### Where tea's config is read from

The first existing file wins:

1. `$TEA_CONFIG`, when set (and then only that file)
2. `$XDG_CONFIG_HOME/tea/config.yml`, when `XDG_CONFIG_HOME` is set
3. `~/.config/tea/config.yml`
4. The OS default location used by tea:

| OS | Path |
|----|------|
| Linux | `~/.config/tea/config.yml` |
| macOS | `~/Library/Application Support/tea/config.yml` |
| Windows | `%LOCALAPPDATA%\tea\config.yml` |

The login whose URL host matches the synced host (port included, e.g. `git.example.org:3000`) is used. If no `tea` login matches a configured host, the host is skipped with a warning and the sync continues with the next one.

### Without tea: a saved token

`tea` is not required. When it is missing, or when you decline it at the prompt, `repos-manager forgejo login` asks for the base URL and an access token (created on the forge under Settings > Applications) and saves them in `~/.config/repos-manager/forgejo-creds`:

{% raw %}
```json
{ "git.example.org": { "url": "https://git.example.org", "token": "..." } }
```
{% endraw %}

A matching `tea` login still wins when both exist, so an existing setup keeps working. The host is only skipped when neither a `tea` login nor a saved token matches it.

### Gogs

Gogs is reached through the same provider, and the saved token is what makes it usable: `tea login add` may refuse a Gogs server, because it cannot probe its version.

Gogs also ignores `page` and `limit` on the repository endpoints and answers the whole list on every request. The listing detects a server that does not advance, warns (`the server ignores 'page', stopped at page N`) and stops, instead of looping forever.

## Bitbucket

**CLI:** [`bitbucket`](https://crates.io/crates/bitbucket-cli) or API fallback ([docs](https://developer.atlassian.com/cloud/bitbucket/rest/intro/))

**Breaking change for existing users:** Atlassian removed app passwords on 2026-07-28, so an app password saved by an older version no longer works. Bitbucket Cloud now needs an **Atlassian API token**, sent as HTTP Basic auth with your **account email** as the user. Log in again.

```bash
# Authenticate (creates ~/.config/repos-manager/bitbucket-creds)
repos-manager bitbucket login

# Sync
repos-manager bitbucket sync
```

`login` asks for the host, `bitbucket.org` by default. On Cloud it then asks for your Atlassian account email and an API token, created at [id.atlassian.com/manage-profile/security/api-tokens](https://id.atlassian.com/manage-profile/security/api-tokens). When the `bitbucket` CLI is installed and the host is `bitbucket.org`, `login` hands over to `bitbucket auth login` instead.

The Cloud listing covers every workspace you are a member of, not just your own. With the API fallback, Bitbucket is also picked up by `sync --all` as soon as the credentials file exists.

### Credentials file

`~/.config/repos-manager/bitbucket-creds` is a JSON map keyed by host (mode `600` on Linux and macOS), so Cloud and several self-hosted instances coexist:

{% raw %}
```json
{
  "bitbucket.org": { "user": "me@example.com", "token": "ATATT..." },
  "bb.corp.example": { "token": "BBDC-..." }
}
```
{% endraw %}

An old single-line `user:password` file is still read as the `bitbucket.org` entry, with a warning telling you to log in again. It is never rewritten behind your back.

### Bitbucket Data Center / Server

Any host other than `bitbucket.org` is treated as Data Center / Server and goes through the 1.0 API (`/rest/api/1.0/repos`) with an **HTTP access token**, created from `profile picture > Manage account > HTTP access tokens`.

```bash
repos-manager bitbucket login                            # host: bb.corp.example
repos-manager bitbucket sync --host bb.corp.example
```

Leave the user empty and the token is sent as a Bearer, which is the only form project- and repository-scoped tokens accept. Set a user and it is sent as Basic.

Repositories land in `<project key lowercased>/<repo slug>`, and personal repositories keep their `~user` project key.

## Radicle

**CLI:** [`rad`](https://radicle.xyz/guides/user) ([docs](https://radicle.xyz/guides/user))

Radicle is a sovereign peer-to-peer code forge. repos-manager lists the repos known to your local Radicle node (`rad ls --json`) and clones them under `<base_dir>/radicle/`.

```bash
# Install rad from https://radicle.xyz
# Authenticate (runs rad auth)
repos-manager radicle login

# Sync
repos-manager radicle sync
```

## Sourcehut

**CLI:** [`hut`](https://sr.ht/~xenrox/hut)

`srht` is accepted as an alias: `repos-manager srht sync` is the same as `repos-manager sourcehut sync`.

```bash
# Install hut from https://sr.ht/~xenrox/hut
# Authenticate (runs hut init)
repos-manager sourcehut login

# Sync
repos-manager sourcehut sync
```

The default host is `git.sr.ht`, so repos are mirrored under `<base_dir>/git.sr.ht/`.

### How listing works

`hut git list` prints only the first page when stdout is not a terminal, and never shows the owner, so repos-manager sends a GraphQL query through `hut graphql git` instead and follows the API cursor until there is no next page.

Private and unlisted repos are included, and a repo shared with you through an ACL appears under its real owner (`~someone-else/repo`).

If `hut` has no token for the instance, the host is skipped with a warning: run `hut init`.

### Names keep the tilde

A sr.ht repository is `~user/repo`, tilde included, so clones land in `<base_dir>/git.sr.ht/~user/repo`. Quote any filter that starts with a tilde, otherwise your shell expands it to your home directory before repos-manager ever sees it:

```bash
repos-manager sourcehut sync --filter '~user/*'
```

Clone URLs carry no `.git` suffix: sr.ht does not serve one.

## OneDev

**CLI:** none. repos-manager talks to the OneDev REST API with an access token.

```bash
# Authenticate (creates ~/.config/repos-manager/onedev-creds)
repos-manager onedev login

# Sync
repos-manager onedev sync
```

`login` prompts for the host, your account name and an access token, created at `https://<host>/~my/access-tokens`.

OneDev is self-hosted only, so it has **no default host**: nothing is synced until you list your instance in the config.

{% raw %}
```json
{ "hosts": { "onedev": ["onedev.example.org"] } }
```
{% endraw %}

### Credentials file

`~/.config/repos-manager/onedev-creds` is a JSON map keyed by host (mode `600` on Linux and macOS), so several instances coexist:

{% raw %}
```json
{ "onedev.example.org": { "user": "me", "token": "..." } }
```
{% endraw %}

### How listing works

The listing pages through `GET /~api/projects?offset=<n>&count=100` with HTTP Basic auth. 100 is the maximum the server accepts for a token that is not an admin one.

Project paths are nested like GitLab groups (`parent/child/project`) and become the local path.

Projects with code management disabled are skipped: in OneDev a project is a tree node, and the parents of a repository often hold no code at all.

### Clone URLs

| Protocol | URL |
|----------|-----|
| HTTPS | `https://<host>/<path>.git` |
| SSH | `ssh://git@<host>:6611/<path>.git` |

OneDev serves git over its own SSH daemon, on port **6611** by default. That port has to be reachable from your machine for the default SSH protocol; use `--https` (or `"protocol": "https"`) otherwise.

**Known limitation:** an instance served under a sub-path (`https://example.org/onedev/`) is not supported, because a configured host is a bare host.

## Plain git servers (`any`)

**CLI:** none, only `git` and `ssh`.

`git` is accepted as an alias: `repos-manager git sync` is the same as `repos-manager any sync`.

For bare repositories served over SSH with no forge API in front of them: gitolite, [Soft Serve](https://github.com/charmbracelet/soft-serve), or a plain directory of `*.git` on a VPS.

```bash
# Nothing to authenticate: these hosts use your own SSH keys
repos-manager any sync
```

These hosts are configured under the top-level `repos` key, whose keys are the hostnames and double as the directory name under `base_dir`. There is no default host: nothing is synced until that key lists something. Four modes are available, and `urls` and `discover` can be combined in one entry:

| Mode | What it does |
|------|--------------|
| `urls` | Clone URLs listed by hand, used exactly as written |
| `discover: "gitolite"` | `ssh [-p port] <user>@<host> info`, with `user` defaulting to `git` |
| `discover: "soft-serve"` | `ssh -p <port> <host> repo list`, with `port` defaulting to 23231 |
| `discover: "find:<root>"` | `find <root> -name '*.git' -prune` over SSH |

See the Configuration page, Plain git servers, for the config snippet of each mode.

The local path comes from the URL, so a listed URL is used as written and `--https` does not rewrite it. `find:<root>` needs a real shell on the remote account: it does not work with `git-shell` unless a `list` script sits in `~/git-shell-commands`.

Every discovery command runs with `BatchMode=yes` and a connect timeout, so an unknown host key or a passphrase with no agent fails with a message instead of blocking the sync.
