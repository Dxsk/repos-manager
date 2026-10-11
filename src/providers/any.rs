//! A plain git server, with no forge API: repository URLs come from the
//! config, or from the servers that can list them over SSH.

use std::process::Command;

use anyhow::{Result, anyhow};

use super::{ListError, Listing, Repo, has_cli, run_capture};
use crate::config::{AnyHost, Settings};
use crate::output;

/// A parallel sync has no one to answer a prompt, so an unknown host key or a
/// passphrase without an agent has to fail instead of waiting forever. Same
/// intent as `git::SSH_COMMAND`, with a connect timeout since these commands
/// are short and the clone that follows sets its own keepalives.
const SSH_OPTS: [&str; 4] = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=10"];

const SOFT_SERVE_PORT: u16 = 23231;
const GITOLITE_USER: &str = "git";

pub fn login() -> Result<()> {
    output::info("any: nothing to authenticate, these hosts are reached with your SSH keys.");
    Ok(())
}

/// The part of a clone URL that follows the host.
fn url_path(url: &str) -> Option<&str> {
    match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/').map(|(_, path)| path),
        // scp-like `[user@]host:path`.
        None => url.split_once(':').map(|(_, path)| path),
    }
}

/// A `..` component would walk the clone out of the host directory.
fn is_safe(name: &str) -> bool {
    !name.split('/').any(|c| c == "..")
}

/// gitolite also reports the config patterns you may create repos under
/// (`CREATOR/..*`, a character class); they are not clonable repositories.
/// A gitolite line may hold a wildcard rule rather than a repo. Only the glob
/// and character-class markers are a reliable signal: `+`, `$` and `^` are
/// regex metacharacters but are also legal in a name (`cpp+exercises`).
fn is_pattern(name: &str) -> bool {
    name.contains("CREATOR") || name.contains(['*', '[', ']', '\\'])
}

/// Where the repo lands under `<base_dir>/<host>/`, from its clone URL.
pub fn full_name(url: &str) -> Option<String> {
    let path = drop_drive(url_path(url)?.trim_matches('/'));
    let name = path.strip_suffix(".git").unwrap_or(path).trim_matches('/');
    (!name.is_empty() && is_safe(name)).then(|| name.to_string())
}

/// A local mirror reached as `file:///C:/mirrors/x.git` would otherwise want a
/// directory named `C:`, which Windows cannot hold.
fn drop_drive(path: &str) -> &str {
    match path.split_once('/') {
        Some((first, rest)) if is_drive(first) => rest,
        // A bare drive letter names no repository.
        None if is_drive(path) => "",
        _ => path,
    }
}

fn is_drive(part: &str) -> bool {
    let bytes = part.as_bytes();
    bytes.len() == 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

fn repo(full_name: &str, url: &str) -> Repo {
    Repo {
        full_name: full_name.to_string(),
        ssh_url: url.to_string(),
        https_url: url.to_string(),
        ..Default::default()
    }
}

/// The configured URLs, kept as written: the user picked the transport, and
/// `--https` must not rewrite it into an address that does not exist.
pub fn from_urls(urls: &[String]) -> Vec<Repo> {
    urls.iter()
        .filter_map(|url| match full_name(url) {
            Some(name) => Some(repo(&name, url)),
            None => {
                output::warn(&format!(
                    "any: cannot read a repo name from '{url}', skipped"
                ));
                None
            }
        })
        .collect()
}

/// The scp-like form carries no port and reads its path as relative to the
/// account, so a custom port or an absolute path forces the `ssh://` form.
fn ssh_url(user: Option<&str>, host: &str, port: Option<u16>, path: &str) -> String {
    let at = user.map(|u| format!("{u}@")).unwrap_or_default();
    match port {
        Some(p) => format!("ssh://{at}{host}:{p}/{}", path.trim_start_matches('/')),
        None if user.is_some() && !path.starts_with('/') => format!("{at}{host}:{path}"),
        None => format!("ssh://{at}{host}/{}", path.trim_start_matches('/')),
    }
}

fn ssh_command(user: Option<&str>, host: &str, port: Option<u16>, args: &[&str]) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.args(SSH_OPTS);
    if let Some(p) = port {
        cmd.arg("-p").arg(p.to_string());
    }
    match user {
        Some(u) => cmd.arg(format!("{u}@{host}")),
        None => cmd.arg(host),
    };
    cmd.args(args);
    cmd
}

/// `ssh git@host info` greets you, then prints one permission line per repo:
/// decorated flag columns (`@C R W`, `@R_ @W_`) and the name.
pub fn gitolite_names(out: &str) -> Vec<String> {
    out.lines().filter_map(gitolite_name).collect()
}

fn gitolite_name(line: &str) -> Option<String> {
    let mut tokens: Vec<&str> = line.split_whitespace().collect();
    let name = tokens.pop()?;
    let flags = !tokens.is_empty() && tokens.iter().all(|t| is_perm_flag(t));
    (flags && is_safe(name) && !is_pattern(name)).then(|| name.to_string())
}

/// `@` means @all holds the permission, `#` a superuser, `&` both, and the `_`
/// suffix that the access is only implicit.
fn is_perm_flag(token: &str) -> bool {
    let flag = token
        .trim_start_matches(['@', '#', '&'])
        .trim_end_matches('_');
    matches!(flag, "C" | "R" | "W")
}

/// `repo list` prints one repository name per line, nothing else.
pub fn soft_serve_names(out: &str) -> Vec<String> {
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains(char::is_whitespace) && is_safe(l))
        .map(|l| l.trim_end_matches(".git").to_string())
        .collect()
}

/// `find <root> -name '*.git' -prune` prints absolute paths; the name is what
/// is left once `<root>` and the suffix are gone.
/// `(name, remote path)`. The path is kept as printed: a repo that is not
/// bare prints `<root>/work/.git`, and rebuilding `<root>/work.git` from the
/// name would point at nothing.
pub fn find_names(out: &str, root: &str) -> Vec<(String, String)> {
    let root = root.trim_end_matches('/');
    out.lines()
        .map(str::trim)
        .filter_map(|path| {
            let rel = path.strip_suffix(".git")?;
            let rel = rel.strip_prefix(root).unwrap_or(rel).trim_matches('/');
            (!rel.is_empty() && is_safe(rel)).then(|| (rel.to_string(), path.to_string()))
        })
        .collect()
}

/// gitolite answers as its own account, `git` by convention.
fn gitolite_user(spec: &AnyHost) -> &str {
    spec.user.as_deref().unwrap_or(GITOLITE_USER)
}

fn gitolite_command(host: &str, spec: &AnyHost) -> Command {
    ssh_command(Some(gitolite_user(spec)), host, spec.port, &["info"])
}

fn gitolite(host: &str, spec: &AnyHost) -> Result<Vec<Repo>, ListError> {
    let raw = run_capture(&mut gitolite_command(host, spec))?;
    let user = Some(gitolite_user(spec));
    Ok(gitolite_names(&raw)
        .iter()
        .map(|name| {
            let url = ssh_url(user, host, spec.port, &format!("{name}.git"));
            repo(name, &url)
        })
        .collect())
}

fn soft_serve_command(host: &str, spec: &AnyHost) -> Command {
    let port = Some(spec.port.unwrap_or(SOFT_SERVE_PORT));
    ssh_command(spec.user.as_deref(), host, port, &["repo", "list"])
}

fn soft_serve(host: &str, spec: &AnyHost) -> Result<Vec<Repo>, ListError> {
    let raw = run_capture(&mut soft_serve_command(host, spec))?;
    let port = Some(spec.port.unwrap_or(SOFT_SERVE_PORT));
    let user = spec.user.as_deref();
    Ok(soft_serve_names(&raw)
        .iter()
        .map(|name| repo(name, &ssh_url(user, host, port, &format!("{name}.git"))))
        .collect())
}

/// ssh joins its arguments back into one command line for the remote shell,
/// so the pattern travels quoted: unquoted it would glob against the remote
/// home directory before find ever sees it.
fn find_command(host: &str, spec: &AnyHost, root: &str) -> Command {
    let args = ["find", root, "-name", "'*.git'", "-prune"];
    ssh_command(spec.user.as_deref(), host, spec.port, &args)
}

/// Needs a real shell on the remote account: a forced git-only command such as
/// gitolite-shell or Soft Serve rejects `find`.
fn find(host: &str, spec: &AnyHost, root: &str) -> Result<Vec<Repo>, ListError> {
    let raw = run_capture(&mut find_command(host, spec, root))?;
    let user = spec.user.as_deref();
    let root = root.trim_end_matches('/');
    Ok(find_names(&raw, root)
        .iter()
        .map(|(name, path)| repo(name, &ssh_url(user, host, spec.port, path)))
        .collect())
}

fn discover(host: &str, spec: &AnyHost, mode: &str) -> Result<Vec<Repo>, ListError> {
    if !has_cli("ssh") {
        return Err(ListError::Skip(format!(
            "ssh not found in PATH, cannot list the repos of '{host}'"
        )));
    }
    match mode.trim() {
        "gitolite" => gitolite(host, spec),
        "soft-serve" => soft_serve(host, spec),
        other => match other.strip_prefix("find:") {
            Some(root) if !root.trim().is_empty() => find(host, spec, root.trim()),
            _ => Err(ListError::Fail(anyhow!(
                "unknown discover mode '{mode}' for '{host}': expected gitolite, soft-serve or find:<root>"
            ))),
        },
    }
}

pub fn list_repos(host: &str, settings: &Settings) -> Result<Listing, ListError> {
    let Some(spec) = settings.any_host(host) else {
        return Err(ListError::Skip(format!(
            "no repos configured for '{host}'. Add them under \"repos\" in the config"
        )));
    };
    let mut repos = from_urls(&spec.urls);
    // A URL that could not be read is missing from the listing, and pruning
    // would then delete the clone it points at.
    let partial = repos.len() < spec.urls.len();
    if let Some(mode) = spec.discover.as_deref() {
        repos.extend(discover(host, spec, mode)?);
    }
    Ok(Listing { repos, partial })
}

#[cfg(test)]
mod tests {
    use super::super::tests::args;
    use super::*;

    #[test]
    fn full_name_from_every_url_form() {
        for (url, want) in [
            ("git@h:infra/ansible.git", "infra/ansible"),
            ("ssh://me@vps:2222/srv/git/notes.git", "srv/git/notes"),
            ("https://h/a/b/c.git", "a/b/c"),
            ("ssh://h/plain", "plain"),
            ("git://h/o/r.git/", "o/r"),
            ("http://h/o/r", "o/r"),
            // A local mirror, which is a plain git server like any other.
            ("file:///srv/git/notes.git", "srv/git/notes"),
            ("file:///C:/mirrors/notes.git", "mirrors/notes"),
            ("file:///c:/notes.git", "notes"),
        ] {
            assert_eq!(full_name(url).as_deref(), Some(want), "{url}");
        }
        for url in [
            "https://h",
            "git@h:",
            "nonsense",
            "git@h:../escape.git",
            "file:///C:/",
            "",
        ] {
            assert_eq!(full_name(url), None, "{url}");
        }
    }

    #[test]
    fn configured_urls_keep_their_transport() {
        let urls = vec![
            "git@h:infra/ansible.git".to_string(),
            "nope".to_string(),
            "https://h/a/b/c.git".to_string(),
        ];
        let repos = from_urls(&urls);
        let names: Vec<_> = repos.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(names, ["infra/ansible", "a/b/c"]);
        assert_eq!(repos[0].ssh_url, "git@h:infra/ansible.git");
        assert_eq!(repos[0].clone_url(true), "git@h:infra/ansible.git");
        assert_eq!(repos[1].clone_url(false), "https://h/a/b/c.git");
    }

    #[test]
    fn gitolite_info_lists_repos_without_the_patterns() {
        let out = "hello sitaram, the gitolite version here is v1.5.5-24-g2b066fc\n\
                   the gitolite config gives you the following access:\n\
                   \u{20}R W\tSecureBrowse\n\
                   @R W    gitolite\n\
                   \u{20}R W\tcpp+exercises\n\
                   @C R W  private/sitaram/[\\w.-]+\n\
                   @C @R W public/sitaram/CREATOR/..*\n\
                   @R_ @W_ testing\n\
                   \n";
        assert_eq!(
            gitolite_names(out),
            // `+` is a regex metacharacter but a legal repo name too.
            ["SecureBrowse", "gitolite", "cpp+exercises", "testing"].map(String::from)
        );
    }

    #[test]
    fn soft_serve_lists_one_repo_per_line() {
        let out = "dotfiles\nsoft-serve\nnested/thing.git\n\n  spaced  \n";
        assert_eq!(
            soft_serve_names(out),
            ["dotfiles", "soft-serve", "nested/thing", "spaced"].map(String::from)
        );
    }

    #[test]
    fn find_paths_become_names_relative_to_the_root() {
        let out = "/srv/git/infra/ansible.git\n/srv/git/notes.git\n/srv/git/work/.git\n\
                   /srv/git/not-a-repo\n/srv/git/../escape.git\n";
        assert_eq!(
            find_names(out, "/srv/git/"),
            [
                ("infra/ansible", "/srv/git/infra/ansible.git"),
                ("notes", "/srv/git/notes.git"),
                // Not bare: the path printed by find is the one to clone.
                ("work", "/srv/git/work/.git"),
            ]
            .map(|(n, p)| (n.to_string(), p.to_string()))
        );
    }

    #[test]
    fn found_repos_clone_from_the_path_find_printed() {
        let out = "/srv/git/notes.git
/srv/git/work/.git
";
        let urls: Vec<String> = find_names(out, "/srv/git")
            .iter()
            .map(|(_, path)| ssh_url(Some("me"), "h", None, path))
            .collect();
        assert_eq!(
            urls,
            [
                "ssh://me@h/srv/git/notes.git",
                "ssh://me@h/srv/git/work/.git"
            ]
        );
    }

    #[test]
    fn clone_urls_match_the_transport() {
        assert_eq!(ssh_url(Some("git"), "h", None, "o/r.git"), "git@h:o/r.git");
        assert_eq!(
            ssh_url(Some("git"), "h", Some(2222), "o/r.git"),
            "ssh://git@h:2222/o/r.git"
        );
        assert_eq!(ssh_url(None, "h", None, "o/r.git"), "ssh://h/o/r.git");
        assert_eq!(
            ssh_url(None, "h", Some(23231), "r.git"),
            "ssh://h:23231/r.git"
        );
        // An absolute path can never take the scp-like form.
        assert_eq!(
            ssh_url(Some("me"), "h", None, "/srv/git/r.git"),
            "ssh://me@h/srv/git/r.git"
        );
    }

    #[test]
    fn ssh_commands_never_prompt() {
        let cmd = ssh_command(Some("git"), "h", None, &["info"]);
        assert_eq!(cmd.get_program(), "ssh");
        assert_eq!(
            args(&cmd),
            [
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=10",
                "git@h",
                "info"
            ]
        );

        let cmd = ssh_command(None, "h", Some(23231), &["repo", "list"]);
        assert_eq!(
            args(&cmd),
            [
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=10",
                "-p",
                "23231",
                "h",
                "repo",
                "list"
            ]
        );
    }

    #[test]
    fn discovery_commands_fill_in_the_defaults() {
        let opts = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=10"];
        let with = |extra: &[&str]| -> Vec<String> {
            opts.iter().chain(extra).map(|s| s.to_string()).collect()
        };

        let bare = AnyHost::default();
        assert_eq!(
            args(&gitolite_command("h", &bare)),
            with(&["git@h", "info"])
        );
        assert_eq!(
            args(&soft_serve_command("h", &bare)),
            with(&["-p", "23231", "h", "repo", "list"])
        );

        let tuned = AnyHost {
            port: Some(2222),
            user: Some("me".into()),
            ..Default::default()
        };
        assert_eq!(
            args(&gitolite_command("h", &tuned)),
            with(&["-p", "2222", "me@h", "info"])
        );
        assert_eq!(
            args(&soft_serve_command("h", &tuned)),
            with(&["-p", "2222", "me@h", "repo", "list"])
        );
        assert_eq!(
            args(&find_command("h", &tuned, "/srv/git")),
            with(&[
                "-p", "2222", "me@h", "find", "/srv/git", "-name", "'*.git'", "-prune"
            ])
        );
    }

    #[test]
    fn unknown_hosts_and_modes_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"repos":{"known.example.org":["git@known.example.org:a/b.git"],
                         "bad.example.org":{"discover":"carrier-pigeon"}}}"#,
        )
        .unwrap();
        let settings = Settings::load_from(&path, |_| None).unwrap();

        let err = list_repos("other.example.org", &settings).unwrap_err();
        assert!(matches!(err, ListError::Skip(m) if m.contains("\"repos\"")));

        let listing = list_repos("known.example.org", &settings).unwrap();
        assert_eq!(listing.repos.len(), 1);
        assert_eq!(listing.repos[0].full_name, "a/b");

        let err = list_repos("bad.example.org", &settings).unwrap_err();
        assert!(
            matches!(&err, ListError::Fail(e) if e.to_string().contains("carrier-pigeon")),
            "{err:?}"
        );
    }

    #[test]
    fn login_is_a_no_op() {
        login().unwrap();
    }

    // Layout from src/commands/info and greeting() in src/lib/Gitolite/Rc.pm of https://github.com/sitaramc/gitolite (permissions, a tab, the repo).
    #[test]
    fn gitolite_3_info_output_lists_only_real_repos() {
        let out = "hello alice, this is git@forge running gitolite3 v3.6.13-8-gd3b2a1f on git 2.43.0\n\
                   \n\
                   \u{20}R W C\tusers/alice/[\\w.-]+\n\
                   \u{20}R W C\tusers/alice/CREATOR/..*\n\
                   \u{20}R W\ttesting\n\
                   \u{20}R\tpublic/docs\n\
                   \u{20}\u{20}\u{20}W\tdropbox\n\
                   \n\
                   Welcome to the forge\n";
        assert_eq!(
            gitolite_names(out),
            ["testing", "public/docs", "dropbox"].map(String::from)
        );
    }

    // Soft Serve prints each visible repository with cmd.Println(r.Name()), see pkg/ssh/cmd/list.go in https://github.com/charmbracelet/soft-serve
    #[test]
    fn soft_serve_list_prints_bare_names() {
        let out = "dotfiles\nsoft-serve\nteam-project\n";
        assert_eq!(
            soft_serve_names(out),
            ["dotfiles", "soft-serve", "team-project"].map(String::from)
        );
    }
}
