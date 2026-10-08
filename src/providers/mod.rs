//! Providers list remote repos by driving their official CLI (gh, glab, tea,
//! bitbucket, rad). Forgejo and the Bitbucket fallback call the HTTP API
//! with credentials the CLI (or `repos-manager bitbucket login`) stored.

mod bitbucket;
mod forgejo;
mod github;
mod gitlab;
mod radicle;

use std::fmt;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    Github,
    Gitlab,
    Forgejo,
    Bitbucket,
    Radicle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// `owner/name`, possibly nested (`group/sub/name`); also the local path under the host dir.
    pub full_name: String,
    pub ssh_url: String,
    pub https_url: String,
}

impl Repo {
    pub fn clone_url(&self, use_https: bool) -> &str {
        if use_https {
            &self.https_url
        } else {
            &self.ssh_url
        }
    }
}

#[derive(Debug)]
pub enum ListError {
    /// Host not usable (not logged in, no credentials): skipped with a warning.
    Skip(String),
    Fail(anyhow::Error),
}

impl From<anyhow::Error> for ListError {
    fn from(e: anyhow::Error) -> Self {
        ListError::Fail(e)
    }
}

impl Provider {
    pub const ALL: [Provider; 5] = [
        Provider::Github,
        Provider::Gitlab,
        Provider::Forgejo,
        Provider::Bitbucket,
        Provider::Radicle,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Provider::Github => "github",
            Provider::Gitlab => "gitlab",
            Provider::Forgejo => "forgejo",
            Provider::Bitbucket => "bitbucket",
            Provider::Radicle => "radicle",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "github" => Some(Provider::Github),
            "gitlab" => Some(Provider::Gitlab),
            "forgejo" | "gitea" => Some(Provider::Forgejo),
            "bitbucket" => Some(Provider::Bitbucket),
            "radicle" => Some(Provider::Radicle),
            _ => None,
        }
    }

    pub fn cli(self) -> &'static str {
        match self {
            Provider::Github => "gh",
            Provider::Gitlab => "glab",
            Provider::Forgejo => "tea",
            Provider::Bitbucket => "bitbucket",
            Provider::Radicle => "rad",
        }
    }

    pub fn default_host(self) -> &'static str {
        match self {
            Provider::Github => "github.com",
            Provider::Gitlab => "gitlab.com",
            Provider::Forgejo => "codeberg.org",
            Provider::Bitbucket => "bitbucket.org",
            Provider::Radicle => "radicle",
        }
    }

    /// Whether this provider can be used on this machine (CLI installed, or a fallback credential).
    pub fn available(self) -> bool {
        has_cli(self.cli()) || (self == Provider::Bitbucket && bitbucket::has_api_creds())
    }

    pub fn login(self) -> Result<()> {
        match self {
            Provider::Github => run_interactive("gh", &["auth", "login"]),
            Provider::Gitlab => run_interactive("glab", &["auth", "login"]),
            Provider::Forgejo => forgejo::login(),
            Provider::Bitbucket => bitbucket::login(),
            Provider::Radicle => radicle::login(),
        }
    }

    pub fn list_repos(self, host: &str) -> Result<Vec<Repo>, ListError> {
        let repos = match self {
            Provider::Github => github::list_repos(host),
            Provider::Gitlab => gitlab::list_repos(host),
            Provider::Forgejo => forgejo::list_repos(host),
            Provider::Bitbucket => bitbucket::list_repos(),
            Provider::Radicle => radicle::list_repos(),
        }?;
        Ok(sorted_unique(repos))
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Org listings can overlap with the user's own repos.
fn sorted_unique(mut repos: Vec<Repo>) -> Vec<Repo> {
    repos.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    repos.dedup_by(|a, b| a.full_name == b.full_name);
    repos
}

pub fn has_cli(name: &str) -> bool {
    which::which(name).is_ok()
}

fn require_cli(name: &str) -> Result<(), ListError> {
    if has_cli(name) {
        Ok(())
    } else {
        Err(ListError::Fail(anyhow!("{name} CLI not found in PATH")))
    }
}

/// Run a command inheriting the terminal (login flows are interactive).
fn run_interactive(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("cannot run {program}: is it installed?"))?;
    if !status.success() {
        bail!("{program} {} failed", args.join(" "));
    }
    Ok(())
}

/// Run a command and return its stdout. stderr is captured and included in the error.
fn run_capture(cmd: &mut Command) -> Result<String> {
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("cannot run {:?}", cmd.get_program()))?;
    if !out.status.success() {
        bail!(
            "{:?} failed: {}",
            cmd.get_program(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `gh api --paginate` and `glab api --paginate` print one JSON array per
/// page back to back (`[...][...]`), which is not a single JSON document.
fn parse_paginated(raw: &str) -> Result<Vec<Value>> {
    let mut all = Vec::new();
    for page in serde_json::Deserializer::from_str(raw).into_iter::<Value>() {
        match page.context("invalid JSON from provider CLI")? {
            Value::Array(items) => all.extend(items),
            other => all.push(other),
        }
    }
    Ok(all)
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn with_git_suffix(url: &str) -> String {
    if url.is_empty() || url.ends_with(".git") {
        url.to_string()
    } else {
        format!("{url}.git")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paginated_arrays_are_merged() {
        let v = parse_paginated("[{\"a\":1}][{\"a\":2}]\n[]").unwrap();
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn git_suffix() {
        assert_eq!(with_git_suffix("https://h/o/r"), "https://h/o/r.git");
        assert_eq!(with_git_suffix("https://h/o/r.git"), "https://h/o/r.git");
    }

    #[test]
    fn names_roundtrip() {
        for p in Provider::ALL {
            assert_eq!(Provider::from_name(p.name()), Some(p));
        }
        assert_eq!(Provider::from_name("gitea"), Some(Provider::Forgejo));
    }

    #[test]
    fn clone_url_protocol() {
        let r = Repo {
            full_name: "o/r".into(),
            ssh_url: "ssh".into(),
            https_url: "https".into(),
        };
        assert_eq!(r.clone_url(false), "ssh");
        assert_eq!(r.clone_url(true), "https");
    }

    #[test]
    fn non_array_pages_and_invalid_json() {
        let v = parse_paginated("{\"a\":1}[{\"a\":2}]").unwrap();
        assert_eq!(v.len(), 2);
        assert!(parse_paginated("").unwrap().is_empty());
        assert!(parse_paginated("[{\"a\":1}][oops").is_err());
    }

    #[test]
    fn provider_metadata() {
        let clis: Vec<_> = Provider::ALL.iter().map(|p| p.cli()).collect();
        assert_eq!(clis, ["gh", "glab", "tea", "bitbucket", "rad"]);
        assert_eq!(Provider::Gitlab.default_host(), "gitlab.com");
        assert_eq!(Provider::Bitbucket.default_host(), "bitbucket.org");
        assert_eq!(Provider::Radicle.to_string(), "radicle");
        assert_eq!(Provider::from_name("sourceforge"), None);
        for p in Provider::ALL {
            // Must not panic whatever is installed.
            let _ = p.available();
        }
    }

    #[test]
    fn listing_is_sorted_and_deduplicated() {
        let repo = |n: &str, url: &str| Repo {
            full_name: n.into(),
            ssh_url: url.into(),
            https_url: url.into(),
        };
        let out = sorted_unique(vec![repo("b/x", "1"), repo("a/y", "2"), repo("b/x", "3")]);
        let names: Vec<_> = out.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(names, ["a/y", "b/x"]);
    }

    #[test]
    fn cli_detection() {
        assert!(has_cli("git"));
        assert!(require_cli("git").is_ok());
        assert!(!has_cli("repos-manager-no-such-cli"));
        let Err(ListError::Fail(e)) = require_cli("repos-manager-no-such-cli") else {
            panic!("expected a failure");
        };
        assert!(e.to_string().contains("not found"));
    }

    #[test]
    fn capture_returns_stdout_or_stderr() {
        let out = run_capture(Command::new("git").arg("--version")).unwrap();
        assert!(out.starts_with("git version"));
        let err = run_capture(Command::new("git").arg("no-such-subcommand")).unwrap_err();
        assert!(err.to_string().contains("failed"), "{err}");
        assert!(run_capture(&mut Command::new("repos-manager-no-such-cli")).is_err());
    }

    #[test]
    fn interactive_reports_failures() {
        run_interactive("git", &["--version"]).unwrap();
        let err = run_interactive("git", &["no-such-subcommand"]).unwrap_err();
        assert_eq!(err.to_string(), "git no-such-subcommand failed");
        let err = run_interactive("repos-manager-no-such-cli", &[]).unwrap_err();
        assert!(err.to_string().contains("is it installed"), "{err}");
    }

    #[test]
    fn list_error_from_anyhow() {
        assert!(matches!(
            ListError::from(anyhow!("boom")),
            ListError::Fail(_)
        ));
    }
}
