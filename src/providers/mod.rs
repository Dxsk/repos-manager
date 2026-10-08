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
        let mut repos = match self {
            Provider::Github => github::list_repos(host),
            Provider::Gitlab => gitlab::list_repos(host),
            Provider::Forgejo => forgejo::list_repos(host),
            Provider::Bitbucket => bitbucket::list_repos(),
            Provider::Radicle => radicle::list_repos(),
        }?;
        repos.sort_by(|a, b| a.full_name.cmp(&b.full_name));
        repos.dedup_by(|a, b| a.full_name == b.full_name);
        Ok(repos)
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
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
}
