//! OneDev has no official CLI: this provider pages through the REST API under
//! `/~api` with a token saved by `repos-manager onedev login`. OneDev is
//! self-hosted only, so credentials are stored per host.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Value, json};

use super::{ListError, Listing, Repo, str_field, with_git_suffix};
use crate::{config, http, output};

/// `GET /~api/projects` answers 406 above 100 unless the token is an admin one.
const PAGE_SIZE: usize = 100;
/// OneDev serves git over its own SSH daemon, on 6611 by default.
const SSH_PORT: u16 = 6611;
/// A server that ignores `offset` would page forever: 10k projects is plenty.
const MAX_PAGES: usize = 100;

fn creds_path() -> PathBuf {
    config::app_config_dir().join("onedev-creds")
}

pub fn has_api_creds() -> bool {
    creds_path().is_file()
}

fn prompt_line(label: &str) -> Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

/// `https://onedev.example.org/sub/` -> `onedev.example.org`, so a pasted URL
/// still matches the bare host of `hosts.onedev`.
fn host_only(input: &str) -> &str {
    let rest = input.split_once("://").map_or(input, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest)
}

/// The web host may carry a port (`host:6610`), the SSH daemon has its own.
fn ssh_host(host: &str) -> &str {
    host.split(':').next().unwrap_or(host)
}

/// `(base url, user, token)`. The base url carries the scheme the login was
/// given, so an instance served over plain HTTP stays reachable.
pub fn creds_for_host(raw: &str, host: &str) -> Option<(String, String, String)> {
    let entry = serde_json::from_str::<Value>(raw).ok()?.get(host)?.clone();
    let (user, token) = (str_field(&entry, "user"), str_field(&entry, "token"));
    let base = match str_field(&entry, "url") {
        url if url.is_empty() => format!("https://{host}"),
        url => url.trim_end_matches('/').to_string(),
    };
    (!user.is_empty() && !token.is_empty()).then_some((base, user, token))
}

/// Keep the entries of the other hosts so several instances can coexist.
fn merged_creds(existing: &str, host: &str, base: &str, user: &str, token: &str) -> String {
    let mut map = match serde_json::from_str(existing) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    };
    map.insert(
        host.to_string(),
        json!({"url": base, "user": user, "token": token}),
    );
    Value::Object(map).to_string()
}

fn save_creds(path: &Path, host: &str, base: &str, user: &str, token: &str) -> Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    fs::create_dir_all(path.parent().expect("creds path has a parent"))?;
    fs::write(path, merged_creds(&existing, host, base, user, token))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn login() -> Result<()> {
    output::info("OneDev is self-hosted: enter the instance to sync.");
    let typed = prompt_line("OneDev host (example: onedev.example.org): ")?;
    let host = host_only(&typed).to_string();
    if host.is_empty() {
        bail!("a host is required");
    }
    // An instance behind plain HTTP has to be reachable too, so the scheme is
    // kept when the login was given one.
    let base = match typed.split_once("://") {
        Some((scheme, _)) => format!("{scheme}://{host}"),
        None => format!("https://{host}"),
    };
    output::info(&format!(
        "Create an access token at: {base}/~my/access-tokens"
    ));
    let user = prompt_line("OneDev account name: ")?;
    if user.is_empty() {
        bail!("an account name is required");
    }
    let token = rpassword::prompt_password("Access token: ")?;
    let token = token.trim();
    if token.is_empty() {
        bail!("an access token is required");
    }

    let path = creds_path();
    save_creds(&path, &host, &base, &user, token)?;
    output::success(&format!(
        "Credentials for {host} saved to {}",
        output::path(&path)
    ));
    Ok(())
}

/// Project paths are nested like GitLab groups (`parent/child/project`).
pub fn parse(host: &str, items: &[Value]) -> Vec<Repo> {
    items
        .iter()
        // A OneDev project is a tree node: the parents of a repo often have no
        // code at all, and cloning those would fail.
        .filter(|p| p.get("codeManagement").and_then(Value::as_bool) != Some(false))
        .map(|p| {
            let path = str_field(p, "path");
            Repo {
                ssh_url: ssh_url(host, &path),
                https_url: https_url(host, &path),
                full_name: path,
                ..Default::default()
            }
        })
        .filter(|r| !r.full_name.is_empty())
        .collect()
}

fn https_url(host: &str, path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    with_git_suffix(&format!("https://{host}/{path}"))
}

fn ssh_url(host: &str, path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    with_git_suffix(&format!("ssh://git@{}:{SSH_PORT}/{path}", ssh_host(host)))
}

pub fn list_repos(host: &str) -> Result<Listing, ListError> {
    let raw = fs::read_to_string(creds_path()).unwrap_or_default();
    let Some((base, user, token)) = creds_for_host(&raw, host) else {
        return Err(ListError::Skip(format!(
            "no credentials for host '{host}'. Run: repos-manager onedev login"
        )));
    };
    Ok(list_from_api(&base, host, &user, &token)?)
}

fn list_from_api(base: &str, host: &str, user: &str, token: &str) -> Result<Listing> {
    let auth = format!("Basic {}", STANDARD.encode(format!("{user}:{token}")));
    let (items, partial) = fetch_pages(base, &auth, MAX_PAGES)?;
    Ok(Listing {
        repos: parse(host, &items),
        partial,
    })
}

/// OneDev paginates with `offset` and `count`, and returns a bare JSON array.
/// `(items, truncated)`. A page cap reached with full pages in hand, or a
/// body that is not a list, both mean entries are missing.
fn fetch_pages(base: &str, auth: &str, max_pages: usize) -> Result<(Vec<Value>, bool)> {
    let agent = http::agent(Duration::from_secs(30));
    let mut all = Vec::new();
    for page in 0..max_pages {
        let url = format!(
            "{base}/~api/projects?offset={}&count={PAGE_SIZE}",
            page * PAGE_SIZE
        );
        let body: Value = agent
            .get(&url)
            .header("Authorization", auth)
            .header("Accept", "application/json")
            .call()
            .context("OneDev API /~api/projects")?
            .body_mut()
            .read_json()?;
        let Value::Array(items) = body else {
            return Ok((all, true));
        };
        let full = items.len() >= PAGE_SIZE;
        all.extend(items);
        if !full {
            return Ok((all, false));
        }
    }
    Ok((all, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::{Response, Server};

    /// Trimmed from a real `GET /~api/projects` answer of code.onedev.io.
    const PAYLOAD: &str = r#"[
        {"id":160,"parentId":270,"name":"server","key":"OD","path":"onedev/server",
         "codeManagement":true,"issueManagement":true},
        {"id":270,"parentId":null,"name":"onedev","key":null,"path":"onedev",
         "codeManagement":false,"issueManagement":false},
        {"id":1050,"parentId":270,"name":"server-ee","path":"onedev/server-ee",
         "codeManagement":true}
    ]"#;

    #[test]
    fn parses_project_paths_and_urls() {
        let items: Vec<Value> = serde_json::from_str(PAYLOAD).unwrap();
        let repos = parse("onedev.example.org", &items);
        let names: Vec<_> = repos.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(names, ["onedev/server", "onedev/server-ee"]);
        assert_eq!(
            repos[0].https_url,
            "https://onedev.example.org/onedev/server.git"
        );
        assert_eq!(
            repos[0].ssh_url,
            "ssh://git@onedev.example.org:6611/onedev/server.git"
        );
    }

    #[test]
    fn parse_drops_pathless_entries_and_keeps_unknown_code_flag() {
        let items: Vec<Value> =
            serde_json::from_str(r#"[{"name":"orphan"},{"path":"a/b"},{"path":""}]"#).unwrap();
        let repos = parse("h", &items);
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].full_name, "a/b");
    }

    #[test]
    fn urls_keep_the_web_port_but_not_for_ssh() {
        assert_eq!(https_url("h:6610", "a/b"), "https://h:6610/a/b.git");
        assert_eq!(ssh_url("h:6610", "a/b"), "ssh://git@h:6611/a/b.git");
        assert_eq!(https_url("h", ""), "");
        assert_eq!(ssh_url("h", ""), "");
    }

    #[test]
    fn host_is_normalized() {
        assert_eq!(
            host_only("https://onedev.example.org/"),
            "onedev.example.org"
        );
        assert_eq!(
            host_only("onedev.example.org:6610"),
            "onedev.example.org:6610"
        );
        assert_eq!(host_only(""), "");
    }

    #[test]
    fn creds_are_merged_per_host() {
        let first = merged_creds("", "a.org", "https://a.org", "me", "t1");
        assert_eq!(creds_for_host(&first, "a.org").unwrap().2, "t1");

        let both = merged_creds(&first, "b.org", "http://b.org", "you", "t2");
        assert_eq!(creds_for_host(&both, "a.org").unwrap().1, "me");
        assert_eq!(
            creds_for_host(&both, "b.org").unwrap(),
            ("http://b.org".into(), "you".into(), "t2".into())
        );
        assert_eq!(creds_for_host(&both, "c.org"), None);

        // An entry written before the url field defaults to HTTPS.
        assert_eq!(
            creds_for_host(r#"{"a.org":{"user":"me","token":"t"}}"#, "a.org").unwrap(),
            ("https://a.org".into(), "me".into(), "t".into())
        );

        // A corrupted file must not lock the user out of saving a new entry.
        assert!(creds_for_host("{oops", "a.org").is_none());
        assert_eq!(
            creds_for_host(
                &merged_creds("{oops", "a.org", "https://a.org", "me", "t"),
                "a.org"
            )
            .unwrap()
            .2,
            "t"
        );
        assert_eq!(creds_for_host(r#"{"a.org":{"user":"me"}}"#, "a.org"), None);
    }

    #[test]
    fn saved_file_is_readable_and_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("onedev-creds");
        save_creds(&path, "a.org", "https://a.org", "me", "t1").unwrap();
        save_creds(&path, "b.org", "https://b.org", "you", "t2").unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert_eq!(creds_for_host(&raw, "a.org").unwrap().2, "t1");
        assert_eq!(creds_for_host(&raw, "b.org").unwrap().2, "t2");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    fn full_page(prefix: &str) -> Response {
        let items: Vec<String> = (0..PAGE_SIZE)
            .map(|i| format!(r#"{{"path":"{prefix}/r{i:03}"}}"#))
            .collect();
        Response::json(&format!("[{}]", items.join(",")))
    }

    #[test]
    fn pages_until_a_short_page() {
        let server = Server::start(|target| match target.rsplit("offset=").next() {
            Some(q) if q.starts_with("0&") => full_page("a"),
            Some(q) if q.starts_with("100&") => Response::json(r#"[{"path":"b/last"}]"#),
            _ => Response::not_found(),
        });

        let listing = list_from_api(&server.url, "h", "me", "secret").unwrap();
        assert_eq!(listing.repos.len(), PAGE_SIZE + 1);
        assert_eq!(listing.repos.last().unwrap().full_name, "b/last");
        assert!(!listing.partial, "a short page ends the listing");

        let auth = format!("Basic {}", STANDARD.encode("me:secret"));
        let reqs = server.requests();
        assert_eq!(reqs.len(), 2);
        assert!(reqs.iter().all(|r| r.contains(&auth)), "{reqs:?}");
        assert!(reqs[0].contains("/~api/projects?offset=0&count=100"));
        assert!(reqs[1].contains("offset=100&count=100"));
    }

    #[test]
    fn a_server_ignoring_offset_is_capped() {
        let server = Server::start(|_| full_page("a"));
        let (items, truncated) = fetch_pages(&server.url, "Basic x", 3).unwrap();
        assert_eq!(items.len(), 3 * PAGE_SIZE);
        assert!(truncated, "the cap cut the listing short");
        assert_eq!(server.requests().len(), 3);
    }

    #[test]
    fn non_array_answers_and_errors() {
        let server = Server::start(|_| Response::json(r#"{"error":"nope"}"#));
        let (items, truncated) = fetch_pages(&server.url, "Basic x", 2).unwrap();
        assert!(items.is_empty());
        assert!(truncated, "an error object is not an exhausted listing");

        let failing = Server::start(|_| Response::not_found());
        let err = list_from_api(&failing.url, "h", "me", "t").unwrap_err();
        assert!(err.to_string().contains("/~api/projects"), "{err:#}");
    }

    // Captured from the public GET https://code.onedev.io/~api/projects?offset=0&count=100, answered without credentials.
    const REAL_PROJECTS: &str = r#"[
        {"id":270,"parentId":null,"forkedFromId":null,"name":"onedev","key":null,
         "path":"onedev","description":null,"createDate":"2019-12-14T00:21:05.000+00:00",
         "codeManagement":false,"wikiManagement":false,"packManagement":true,
         "issueManagement":false,"timeTracking":false,"serviceDeskEmailAddress":null,
         "gitPackConfig":{"windowMemory":null,"packSizeLimit":null,"threads":null,"window":null},
         "codeIndexingSetting":{"analysisFiles":null,"requireLoginForAutoIndexing":null}},
        {"id":160,"parentId":270,"forkedFromId":null,"name":"server","key":"OD",
         "path":"onedev/server","description":"The Unified and Autonomous Development Platform",
         "createDate":"2019-12-14T00:21:05.000+00:00","codeManagement":true,
         "wikiManagement":false,"packManagement":true,"issueManagement":true,
         "timeTracking":false,"serviceDeskEmailAddress":null,
         "gitPackConfig":{"windowMemory":null,"packSizeLimit":null,"threads":null,"window":null},
         "codeIndexingSetting":{"analysisFiles":"-**/src/main/java/org/** -**/web/asset/**","requireLoginForAutoIndexing":null}},
        {"id":1050,"parentId":270,"forkedFromId":null,"name":"server-ee","key":null,
         "path":"onedev/server-ee","description":null,
         "createDate":"2024-01-21T09:56:59.586+00:00","codeManagement":true,
         "wikiManagement":false,"packManagement":false,"issueManagement":false,
         "timeTracking":false,"serviceDeskEmailAddress":null,
         "gitPackConfig":{"windowMemory":null,"packSizeLimit":null,"threads":null,"window":null},
         "codeIndexingSetting":{"analysisFiles":null,"requireLoginForAutoIndexing":null}}
    ]"#;

    #[test]
    fn full_project_objects_skip_the_code_less_parent() {
        let items: Vec<Value> = serde_json::from_str(REAL_PROJECTS).unwrap();
        let repos = parse("code.onedev.io", &items);
        let names: Vec<_> = repos.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(names, ["onedev/server", "onedev/server-ee"]);
    }

    #[test]
    fn full_project_objects_through_the_http_listing() {
        let server = Server::start(|_| Response::json(REAL_PROJECTS));
        let listing = list_from_api(&server.url, "code.onedev.io", "me", "secret").unwrap();
        assert_eq!(listing.repos.len(), 2);
        assert_eq!(
            listing.repos[0].https_url,
            "https://code.onedev.io/onedev/server.git"
        );
        assert!(!listing.partial);
    }
}
