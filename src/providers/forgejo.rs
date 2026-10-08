//! Forgejo / Gitea. `tea` is only used for `login add`: `tea repo list` does
//! not enumerate organizations, so listing reads the token from tea's config
//! and pages through the REST API directly.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use ureq::Agent;

use super::{ListError, Repo, has_cli, run_interactive, str_field, with_git_suffix};
use crate::config::home_dir;
use crate::http;

const PAGE_SIZE: usize = 50;

pub fn login() -> Result<()> {
    if !has_cli("tea") {
        bail!("tea CLI not found. Install it: https://gitea.com/gitea/tea");
    }
    run_interactive("tea", &["login", "add"])
}

/// tea stores its config under the XDG config dir, which resolves to
/// `~/.config` on Linux, `~/Library/Application Support` on macOS and
/// `%LOCALAPPDATA%` on Windows.
fn tea_config_candidates(env: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    if let Some(p) = env("TEA_CONFIG") {
        return vec![PathBuf::from(p)];
    }
    let mut out = Vec::new();
    if let Some(x) = env("XDG_CONFIG_HOME") {
        out.push(PathBuf::from(x).join("tea").join("config.yml"));
    }
    out.push(home_dir().join(".config").join("tea").join("config.yml"));
    if let Some(d) = dirs::config_local_dir() {
        out.push(d.join("tea").join("config.yml"));
    }
    out
}

/// Strip scheme, path and trailing slash: `https://git.example.org/` -> `git.example.org`.
fn url_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest)
}

/// Find the `(base_url, token)` of the tea login matching `host`.
pub fn creds_for_host(config_yaml: &str, host: &str) -> Option<(String, String)> {
    let doc: Value = serde_yaml_ng::from_str(config_yaml).ok()?;
    doc.get("logins")?.as_array()?.iter().find_map(|login| {
        let url = login.get("url")?.as_str()?;
        let token = login.get("token")?.as_str()?;
        (!token.is_empty() && url_host(url) == host)
            .then(|| (url.trim_end_matches('/').to_string(), token.to_string()))
    })
}

fn paginate(agent: &Agent, base: &str, token: &str, path: &str) -> Result<Vec<Value>> {
    let sep = if path.contains('?') { '&' } else { '?' };
    let mut all = Vec::new();
    for page in 1.. {
        let url = format!("{base}{path}{sep}limit={PAGE_SIZE}&page={page}");
        let items: Value = agent
            .get(&url)
            .header("Authorization", format!("token {token}"))
            .header("Accept", "application/json")
            .call()
            .with_context(|| format!("Forgejo API {path}"))?
            .body_mut()
            .read_json()?;
        let Value::Array(items) = items else { break };
        let n = items.len();
        all.extend(items);
        if n < PAGE_SIZE {
            break;
        }
    }
    Ok(all)
}

pub fn parse(items: &[Value]) -> Vec<Repo> {
    items
        .iter()
        .map(|r| Repo {
            full_name: str_field(r, "full_name"),
            ssh_url: str_field(r, "ssh_url"),
            https_url: with_git_suffix(&str_field(r, "html_url")),
        })
        .filter(|r| !r.full_name.is_empty())
        .collect()
}

pub fn list_repos(host: &str) -> Result<Vec<Repo>, ListError> {
    let Some(config) = tea_config_candidates(|k| std::env::var_os(k))
        .into_iter()
        .find(|p| p.is_file())
    else {
        return Err(ListError::Skip(
            "tea config not found. Run: tea login add".into(),
        ));
    };
    let yaml = std::fs::read_to_string(&config).map_err(anyhow::Error::from)?;
    let Some((base, token)) = creds_for_host(&yaml, host) else {
        return Err(ListError::Skip(format!(
            "no tea login matches host '{host}'. Run: tea login add"
        )));
    };
    Ok(list_from_api(&base, &token)?)
}

/// The user's own repos plus those of every org they belong to.
fn list_from_api(base: &str, token: &str) -> Result<Vec<Repo>> {
    let agent = http::agent(Duration::from_secs(30));
    let mut items = paginate(&agent, base, token, "/api/v1/user/repos")?;
    for org in paginate(&agent, base, token, "/api/v1/user/orgs")? {
        let name = org
            .get("username")
            .or_else(|| org.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        match paginate(&agent, base, token, &format!("/api/v1/orgs/{name}/repos")) {
            Ok(org_items) => items.extend(org_items),
            Err(e) => crate::output::warn(&format!("org {name}: {e:#}")),
        }
    }
    Ok(parse(&items))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::{Response, Server};
    use std::path::Path;

    const CONFIG: &str = r#"
logins:
  - name: codeberg
    url: https://codeberg.org
    token: tok1
  - name: self
    url: https://git.example.org/
    token: tok2
  - name: empty
    url: https://empty.org
    token: ""
"#;

    #[test]
    fn matches_host_and_strips_slash() {
        assert_eq!(
            creds_for_host(CONFIG, "git.example.org"),
            Some(("https://git.example.org".into(), "tok2".into()))
        );
        assert_eq!(creds_for_host(CONFIG, "codeberg.org").unwrap().1, "tok1");
    }

    #[test]
    fn unknown_host_or_empty_token() {
        assert_eq!(creds_for_host(CONFIG, "nope.org"), None);
        assert_eq!(creds_for_host(CONFIG, "empty.org"), None);
        assert_eq!(creds_for_host("not: [valid", "x"), None);
    }

    #[test]
    fn host_extraction() {
        assert_eq!(url_host("https://a.org/sub/path"), "a.org");
        assert_eq!(url_host("http://a.org:3000"), "a.org:3000");
    }

    #[test]
    fn config_candidates_follow_env() {
        let explicit = tea_config_candidates(|k| (k == "TEA_CONFIG").then(|| "/x/tea.yml".into()));
        assert_eq!(explicit, [PathBuf::from("/x/tea.yml")]);

        let xdg = tea_config_candidates(|k| (k == "XDG_CONFIG_HOME").then(|| "/xdg".into()));
        assert_eq!(xdg[0], Path::new("/xdg").join("tea").join("config.yml"));
        assert_eq!(
            xdg[1],
            home_dir().join(".config").join("tea").join("config.yml")
        );

        let none = tea_config_candidates(|_| None);
        assert_eq!(
            none[0],
            home_dir().join(".config").join("tea").join("config.yml")
        );
    }

    fn repo(name: &str) -> String {
        format!(
            r#"{{"full_name":"{name}","ssh_url":"git@h:{name}.git","html_url":"https://h/{name}"}}"#
        )
    }

    fn page(names: &[String]) -> Response {
        Response::json(&format!("[{}]", names.join(",")))
    }

    #[test]
    fn lists_user_and_org_repos_across_pages() {
        let server = Server::start(|target| {
            let (path, query) = target.split_once('?').unwrap();
            let page_no = query.rsplit("page=").next().unwrap();
            match (path, page_no) {
                ("/api/v1/user/repos", "1") => page(
                    &(0..PAGE_SIZE)
                        .map(|i| repo(&format!("me/r{i:02}")))
                        .collect::<Vec<_>>(),
                ),
                ("/api/v1/user/repos", "2") => page(&[repo("me/last")]),
                ("/api/v1/user/orgs", _) => Response::json(
                    r#"[{"username":"org1"},{"name":"org2"},{"username":""},{"username":"broken"}]"#,
                ),
                ("/api/v1/orgs/org1/repos", _) => page(&[repo("org1/a")]),
                ("/api/v1/orgs/org2/repos", _) => Response::json(r#"{"message":"not a list"}"#),
                _ => Response::not_found(),
            }
        });

        let repos = list_from_api(&server.url, "secret").unwrap();
        assert_eq!(repos.len(), PAGE_SIZE + 2);
        assert!(repos.iter().any(|r| r.full_name == "me/last"));
        let org = repos.iter().find(|r| r.full_name == "org1/a").unwrap();
        assert_eq!(org.https_url, "https://h/org1/a.git");

        let reqs = server.requests();
        assert!(reqs.iter().all(|r| r.contains("token secret")));
        assert!(reqs.iter().any(|r| r.contains("/api/v1/orgs/broken/repos")));
    }

    #[test]
    fn api_error_fails_listing() {
        let server = Server::start(|_| Response::not_found());
        assert!(list_from_api(&server.url, "t").is_err());
    }

    #[test]
    fn parse_drops_nameless_entries() {
        let items: Vec<Value> =
            serde_json::from_str(r#"[{"ssh_url":"x"},{"full_name":"o/r"}]"#).unwrap();
        let repos = parse(&items);
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].https_url, "");
    }
}
