//! Forgejo / Gitea / Gogs. `tea` is only used for `login add`: `tea repo list`
//! does not enumerate organizations, so listing reads the token from tea's
//! config (or from the creds file written by `repos-manager forgejo login`)
//! and pages through the REST API directly.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use ureq::Agent;

use super::{
    ListError, Listing, Repo, has_cli, opt_u64_field, run_interactive, str_field, with_git_suffix,
};
use crate::config::{app_config_dir, home_dir};
use crate::{http, output};

const PAGE_SIZE: usize = 50;
/// Gogs ignores `page` on the repo listings and answers the whole list every
/// time, so pagination needs a hard stop on top of the repeat detection.
const MAX_PAGES: usize = 200;

fn api_creds_path() -> PathBuf {
    app_config_dir().join("forgejo-creds")
}

/// Meant for `Provider::available()`: the fallback creds make a Gogs host
pub fn has_api_creds() -> bool {
    api_creds_path().is_file()
}

pub fn login() -> Result<()> {
    if has_cli("tea") {
        let answer = prompt_line("tea CLI found, use `tea login add`? [Y/n]: ")?;
        if !answer.eq_ignore_ascii_case("n") && !answer.eq_ignore_ascii_case("no") {
            return run_interactive("tea", &["login", "add"]);
        }
    } else {
        output::info("No tea CLI found. Using direct API auth.");
    }
    api_login()
}

fn prompt_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

/// `tea login add` rejects servers whose version it cannot probe (Gogs), so the
/// base URL and the token are stored here instead, keyed by host.
fn api_login() -> Result<()> {
    output::info("Works with Forgejo, Gitea and Gogs.");
    output::info("Create a token on the forge under Settings > Applications.");
    let url = normalize_base(&prompt_line("Base URL (https://git.example.org): ")?);
    let host = url_host(&url);
    if host.is_empty() {
        bail!("a base URL is required");
    }
    let token = rpassword::prompt_password("Access token: ")?;
    let token = token.trim();
    if token.is_empty() {
        bail!("an access token is required");
    }

    let path = api_creds_path();
    fs::create_dir_all(path.parent().expect("creds path has a parent"))?;
    let existing = fs::read_to_string(&path).unwrap_or_default();
    fs::write(&path, merge_creds(&existing, host, &url, token))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    output::success(&format!(
        "Credentials for {host} saved to {}",
        output::path(&path)
    ));
    Ok(())
}

fn normalize_base(input: &str) -> String {
    let url = input.trim().trim_end_matches('/');
    if url.contains("://") {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

/// Rewrite the whole map so logging into one host keeps the others.
fn merge_creds(existing: &str, host: &str, url: &str, token: &str) -> String {
    let mut map = match serde_json::from_str::<Value>(existing) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    };
    map.insert(
        host.to_string(),
        serde_json::json!({"url": url, "token": token}),
    );
    serde_json::to_string_pretty(&Value::Object(map)).expect("a JSON map serializes")
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

/// Find the `(base_url, token)` saved by `api_login()` for `host`.
fn api_creds_for_host(json: &str, host: &str) -> Option<(String, String)> {
    let doc: Value = serde_json::from_str(json).ok()?;
    let entry = doc.get(host)?;
    let token = entry.get("token").and_then(Value::as_str)?;
    let url = entry
        .get("url")
        .and_then(Value::as_str)
        .map(|u| u.trim_end_matches('/').to_string())
        .unwrap_or_else(|| format!("https://{host}"));
    (!token.is_empty()).then(|| (url, token.to_string()))
}

/// A tea login wins over the creds file, so an existing setup keeps working.
fn creds_lookup(
    tea_config: Option<&Path>,
    api_creds: &Path,
    host: &str,
) -> Result<(String, String), ListError> {
    if let Some(path) = tea_config {
        let yaml = fs::read_to_string(path).map_err(anyhow::Error::from)?;
        if let Some(creds) = creds_for_host(&yaml, host) {
            return Ok(creds);
        }
    }
    if let Some(creds) = fs::read_to_string(api_creds)
        .ok()
        .and_then(|json| api_creds_for_host(&json, host))
    {
        return Ok(creds);
    }
    let how = "Run: tea login add, or repos-manager forgejo login";
    // The "tea config not found" wording is asserted by the CLI tests.
    Err(ListError::Skip(match tea_config {
        None => format!("tea config not found and no saved credentials for '{host}'. {how}"),
        Some(_) => format!("no tea login or saved credentials for host '{host}'. {how}"),
    }))
}

/// `None` when the payload is not a JSON array (an error object, for instance).
fn fetch_page(agent: &Agent, url: &str, token: &str, path: &str) -> Result<Option<Vec<Value>>> {
    let body: Value = agent
        .get(url)
        .header("Authorization", format!("token {token}"))
        .header("Accept", "application/json")
        .call()
        .with_context(|| format!("Forgejo API {path}"))?
        .body_mut()
        .read_json()?;
    Ok(match body {
        Value::Array(items) => Some(items),
        _ => None,
    })
}

/// Identity of each item, to spot a server answering the same page again:
/// `id` when present, else `full_name`.
fn item_ids(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item.get("id") {
            Some(id) => id.to_string(),
            None => str_field(item, "full_name"),
        })
        .collect()
}

/// `(items, truncated)`. Truncated means the cap was hit with a full page in
/// hand, so entries are missing.
fn paginate(agent: &Agent, base: &str, token: &str, path: &str) -> Result<(Vec<Value>, bool)> {
    let sep = if path.contains('?') { '&' } else { '?' };
    let mut all = Vec::new();
    let mut previous: Option<Vec<String>> = None;
    for page in 1..=MAX_PAGES {
        let url = format!("{base}{path}{sep}limit={PAGE_SIZE}&page={page}");
        let Some(items) = fetch_page(agent, &url, token, path)? else {
            break;
        };
        // A server may answer more than asked (Gogs sends everything at once).
        let full_page = items.len() >= PAGE_SIZE;
        let ids = item_ids(&items);
        if previous.as_ref() == Some(&ids) {
            output::warn(&format!(
                "{path}: the server ignores 'page', stopped at page {page}"
            ));
            break;
        }
        previous = Some(ids);
        all.extend(items);
        if !full_page {
            break;
        }
        if page == MAX_PAGES {
            output::warn(&format!(
                "{path}: stopped after {MAX_PAGES} pages, the listing may be incomplete"
            ));
            return Ok((all, true));
        }
    }
    Ok((all, false))
}

pub fn parse(items: &[Value]) -> Vec<Repo> {
    items
        .iter()
        .map(|r| Repo {
            full_name: str_field(r, "full_name"),
            ssh_url: str_field(r, "ssh_url"),
            https_url: with_git_suffix(&str_field(r, "html_url")),
            size_kb: opt_u64_field(r, "size"),
        })
        .filter(|r| !r.full_name.is_empty())
        .collect()
}

pub fn list_repos(host: &str) -> Result<Listing, ListError> {
    let tea_config = tea_config_candidates(|k| std::env::var_os(k))
        .into_iter()
        .find(|p| p.is_file());
    let (base, token) = creds_lookup(tea_config.as_deref(), &api_creds_path(), host)?;
    Ok(list_from_api(&base, &token)?)
}

/// The user's own repos plus those of every org they belong to.
fn list_from_api(base: &str, token: &str) -> Result<Listing> {
    let agent = http::agent(Duration::from_secs(30));
    let (mut items, mut partial) = paginate(&agent, base, token, "/api/v1/user/repos")?;
    let (orgs, orgs_partial) = paginate(&agent, base, token, "/api/v1/user/orgs")?;
    partial |= orgs_partial;
    for org in orgs {
        let name = org
            .get("username")
            .or_else(|| org.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        match paginate(&agent, base, token, &format!("/api/v1/orgs/{name}/repos")) {
            Ok((org_items, org_partial)) => {
                items.extend(org_items);
                partial |= org_partial;
            }
            // The org's repos are missing from the listing, so pruning it
            // would delete them.
            Err(e) => {
                output::warn(&format!("org {name}: {e:#}"));
                partial = true;
            }
        }
    }
    Ok(Listing {
        repos: parse(&items),
        partial,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::{Response, Server};

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

    const API_CREDS: &str = r#"{
      "gogs.example.org": {"url": "http://gogs.example.org:3000/", "token": "gtok"},
      "git.example.org": {"url": "https://git.example.org", "token": "filetok"}
    }"#;

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

    fn creds_files() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let tea = dir.path().join("config.yml");
        let api = dir.path().join("forgejo-creds");
        fs::write(&tea, CONFIG).unwrap();
        fs::write(&api, API_CREDS).unwrap();
        (dir, tea, api)
    }

    #[test]
    fn creds_file_covers_hosts_tea_does_not() {
        let (_dir, tea, api) = creds_files();
        let creds = creds_lookup(Some(&tea), &api, "gogs.example.org").unwrap();
        assert_eq!(
            creds,
            ("http://gogs.example.org:3000".into(), "gtok".into())
        );
    }

    #[test]
    fn tea_login_wins_over_creds_file() {
        let (_dir, tea, api) = creds_files();
        let with_tea = creds_lookup(Some(&tea), &api, "git.example.org").unwrap();
        assert_eq!(with_tea.1, "tok2");
        let without_tea = creds_lookup(None, &api, "git.example.org").unwrap();
        assert_eq!(without_tea.1, "filetok");
    }

    #[test]
    fn host_in_no_source_is_skipped() {
        let (dir, tea, api) = creds_files();
        assert!(matches!(
            creds_lookup(Some(&tea), &api, "nope.org"),
            Err(ListError::Skip(_))
        ));
        let missing = dir.path().join("absent");
        assert!(matches!(
            creds_lookup(None, &missing, "nope.org"),
            Err(ListError::Skip(_))
        ));
    }

    #[test]
    fn creds_merge_keeps_other_hosts() {
        let merged = merge_creds(API_CREDS, "new.org", "https://new.org", "ntok");
        assert_eq!(
            api_creds_for_host(&merged, "new.org"),
            Some(("https://new.org".into(), "ntok".into()))
        );
        assert_eq!(
            api_creds_for_host(&merged, "gogs.example.org").unwrap().1,
            "gtok"
        );

        // A corrupt file is replaced instead of failing the login.
        let fresh = merge_creds("not json", "a.org", "https://a.org", "t");
        assert_eq!(
            api_creds_for_host(&fresh, "a.org").unwrap().0,
            "https://a.org"
        );
        assert_eq!(api_creds_for_host(&fresh, "gogs.example.org"), None);
    }

    #[test]
    fn api_creds_defaults_and_rejections() {
        assert_eq!(
            api_creds_for_host(r#"{"h":{"token":"t"}}"#, "h"),
            Some(("https://h".into(), "t".into()))
        );
        assert_eq!(api_creds_for_host(r#"{"h":{"token":""}}"#, "h"), None);
        assert_eq!(
            api_creds_for_host(r#"{"h":{"url":"https://h"}}"#, "h"),
            None
        );
        assert_eq!(api_creds_for_host("[]", "h"), None);
        assert_eq!(api_creds_for_host("", "h"), None);
    }

    #[test]
    fn base_url_gets_a_scheme() {
        assert_eq!(
            normalize_base(" git.example.org/ "),
            "https://git.example.org"
        );
        assert_eq!(
            normalize_base("http://localhost:3000"),
            "http://localhost:3000"
        );
    }

    #[test]
    fn creds_path_sits_next_to_the_config() {
        let path = api_creds_path();
        let dir = app_config_dir();
        assert_eq!(path.parent(), Some(dir.as_path()));
        assert!(path.ends_with("forgejo-creds"));
        // Must not panic whatever the machine holds.
        let _ = has_api_creds();
    }

    #[test]
    fn item_identity_prefers_id() {
        let items: Vec<Value> =
            serde_json::from_str(r#"[{"id":7,"full_name":"o/r"},{"full_name":"o/s"},{}]"#).unwrap();
        assert_eq!(item_ids(&items), ["7", "o/s", ""]);
    }

    fn repo(name: &str) -> String {
        format!(
            r#"{{"full_name":"{name}","ssh_url":"git@h:{name}.git","html_url":"https://h/{name}"}}"#
        )
    }

    fn page(names: &[String]) -> Response {
        Response::json(&format!("[{}]", names.join(",")))
    }

    fn full_page(prefix: &str) -> Response {
        page(
            &(0..PAGE_SIZE)
                .map(|i| repo(&format!("{prefix}/r{i:02}")))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn lists_user_and_org_repos_across_pages() {
        let server = Server::start(|target| {
            let (path, query) = target.split_once('?').unwrap();
            let page_no = query.rsplit("page=").next().unwrap();
            match (path, page_no) {
                ("/api/v1/user/repos", "1") => full_page("me"),
                ("/api/v1/user/repos", "2") => page(&[repo("me/last")]),
                ("/api/v1/user/orgs", _) => Response::json(
                    r#"[{"username":"org1"},{"name":"org2"},{"username":""},{"username":"broken"}]"#,
                ),
                ("/api/v1/orgs/org1/repos", _) => page(&[repo("org1/a")]),
                ("/api/v1/orgs/org2/repos", _) => Response::json(r#"{"message":"not a list"}"#),
                _ => Response::not_found(),
            }
        });

        let listing = list_from_api(&server.url, "secret").unwrap();
        let repos = &listing.repos;
        assert_eq!(repos.len(), PAGE_SIZE + 2);
        assert!(repos.iter().any(|r| r.full_name == "me/last"));
        let org = repos.iter().find(|r| r.full_name == "org1/a").unwrap();
        assert_eq!(org.https_url, "https://h/org1/a.git");
        // One org answered an error object and another 404ed: their repos are
        // missing from the listing.
        assert!(listing.partial);

        let reqs = server.requests();
        assert!(reqs.iter().all(|r| r.contains("token secret")));
        assert!(reqs.iter().any(|r| r.contains("/api/v1/orgs/broken/repos")));
    }

    /// Gogs answers the whole list whatever `page` says: the repeated page must
    /// stop the loop instead of spinning forever.
    #[test]
    fn server_ignoring_page_terminates() {
        let server = Server::start(|target| {
            if target.starts_with("/api/v1/user/orgs") {
                Response::json("[]")
            } else if target.starts_with("/api/v1/user/repos") {
                full_page("me")
            } else {
                Response::not_found()
            }
        });

        let listing = list_from_api(&server.url, "t").unwrap();
        assert_eq!(listing.repos.len(), PAGE_SIZE);
        // The whole list came back at once, so nothing is missing.
        assert!(!listing.partial);
        // Two identical repo pages plus the orgs page, then it gives up.
        assert_eq!(server.requests().len(), 3);
    }

    /// A server that keeps inventing new full pages is stopped by the cap.
    #[test]
    fn page_cap_bounds_an_endless_server() {
        let server = Server::start(|target| {
            let page_no = target.rsplit("page=").next().unwrap_or("0").to_string();
            full_page(&format!("p{page_no}"))
        });

        let agent = http::agent(Duration::from_secs(10));
        let (items, truncated) = paginate(&agent, &server.url, "t", "/api/v1/user/repos").unwrap();
        assert_eq!(items.len(), MAX_PAGES * PAGE_SIZE);
        assert!(truncated, "the cap cut the listing short");
        assert_eq!(server.requests().len(), MAX_PAGES);
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
