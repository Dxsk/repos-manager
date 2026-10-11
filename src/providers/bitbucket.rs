//! Bitbucket: uses the `bitbucket` CLI when installed on Cloud, otherwise the
//! REST API with a token saved by `repos-manager bitbucket login`. Cloud goes
//! through the 2.0 API with an Atlassian API token (app passwords were removed
//! on 2026-07-28), Data Center through the 1.0 API with an HTTP access token.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ureq::Agent;

use super::{
    ListError, Listing, Repo, has_cli, opt_u64_field, run_capture, run_interactive, str_field,
};
use crate::{config, http, output};

const CLOUD_HOST: &str = "bitbucket.org";
const PAGE_SIZE: usize = 100;
/// A server that keeps handing out a next page must not hold a sync forever.
const MAX_PAGES: usize = 500;
const TOKEN_URL: &str = "https://id.atlassian.com/manage-profile/security/api-tokens";

/// One stored credential. `user` is the Atlassian account email on Cloud and is
/// absent for a Data Center token meant to be sent as a Bearer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Creds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user: Option<String>,
    token: String,
}

fn creds_path() -> PathBuf {
    config::app_config_dir().join("bitbucket-creds")
}

/// `(credentials, legacy)`, where `legacy` flags the pre-API-token single
/// `user:pass` line so the caller can warn without rewriting the file.
fn parse_creds(raw: &str) -> (BTreeMap<String, Creds>, bool) {
    if let Ok(map) = serde_json::from_str(raw) {
        return (map, false);
    }
    let legacy = legacy_creds(raw.trim());
    let is_legacy = !legacy.is_empty();
    (legacy, is_legacy)
}

fn legacy_creds(line: &str) -> BTreeMap<String, Creds> {
    let Some((user, token)) = line.split_once(':') else {
        return BTreeMap::new();
    };
    if user.is_empty() || token.is_empty() {
        return BTreeMap::new();
    }
    BTreeMap::from([(
        CLOUD_HOST.to_string(),
        Creds {
            user: Some(user.to_string()),
            token: token.to_string(),
        },
    )])
}

pub fn has_api_creds() -> bool {
    fs::read_to_string(creds_path()).is_ok_and(|raw| !parse_creds(&raw).0.is_empty())
}

fn creds_for_host(host: &str) -> Option<Creds> {
    let raw = fs::read_to_string(creds_path()).ok()?;
    let (map, legacy) = parse_creds(&raw);
    if legacy {
        output::warn(
            "bitbucket-creds holds an app password, which no longer works. \
             Run: repos-manager bitbucket login",
        );
    }
    map.get(host).cloned()
}

fn prompt_line(label: &str, default: &str) -> Result<String> {
    if default.is_empty() {
        print!("{label}: ");
    } else {
        print!("{label} [{default}]: ");
    }
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let line = line.trim();
    Ok(if line.is_empty() {
        default.to_string()
    } else {
        line.to_string()
    })
}

fn prompt_creds(host: &str) -> Result<Creds> {
    let cloud = host == CLOUD_HOST;
    if cloud {
        output::info("Bitbucket Cloud authenticates with an Atlassian API token sent as");
        output::info("HTTP Basic auth: the account email as user, the token as password.");
        output::info(&format!("Create one at: {TOKEN_URL}"));
    } else {
        output::info("Bitbucket Data Center authenticates with an HTTP access token.");
        output::info("Create one from: profile picture > Manage account > HTTP access tokens.");
        output::info("Leave the user empty to send the token as a Bearer.");
    }

    let user = prompt_line(
        if cloud {
            "Atlassian account email"
        } else {
            "User (empty for a Bearer token)"
        },
        "",
    )?;
    if cloud && user.is_empty() {
        bail!("Bitbucket Cloud needs the Atlassian account email as the Basic auth user");
    }
    let token = rpassword::prompt_password(if cloud {
        "API token: "
    } else {
        "HTTP access token: "
    })?;
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("no token entered");
    }
    Ok(Creds {
        user: (!user.is_empty()).then_some(user),
        token,
    })
}

/// Merge `entry` into the map held in `raw`, keeping the entries of other
/// hosts. A legacy app password is not carried over: it cannot authenticate
/// anything any more, and keeping it would turn the warning into a 401.
fn merged_creds(raw: &str, host: &str, entry: Creds) -> Result<String> {
    let (mut map, legacy) = parse_creds(raw);
    if legacy {
        map.clear();
    }
    map.insert(host.to_string(), entry);
    Ok(serde_json::to_string_pretty(&map)?)
}

fn save_creds(host: &str, entry: Creds) -> Result<()> {
    let path = creds_path();
    let raw = fs::read_to_string(&path).unwrap_or_default();
    let merged = merged_creds(&raw, host, entry)?;
    fs::create_dir_all(path.parent().expect("creds path has a parent"))?;
    fs::write(&path, merged)?;
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

pub fn login() -> Result<()> {
    let host = prompt_line("Bitbucket host", CLOUD_HOST)?;
    if host == CLOUD_HOST && has_cli("bitbucket") {
        return run_interactive("bitbucket", &["auth", "login"]);
    }
    save_creds(&host, prompt_creds(&host)?)
}

/// Data Center project and repository tokens only work as a Bearer; a stored
/// user means Basic, which is what Cloud API tokens need.
fn auth_header(creds: &Creds) -> String {
    match creds.user.as_deref().filter(|u| !u.is_empty()) {
        Some(user) => format!(
            "Basic {}",
            STANDARD.encode(format!("{user}:{}", creds.token))
        ),
        None => format!("Bearer {}", creds.token),
    }
}

fn clone_link(repo: &Value, kind: &str) -> String {
    repo.pointer("/links/clone")
        .and_then(Value::as_array)
        .and_then(|links| {
            links
                .iter()
                .find(|l| l.get("name").and_then(Value::as_str) == Some(kind))
        })
        .map(|l| str_field(l, "href"))
        .unwrap_or_default()
}

pub fn parse(items: &[Value]) -> Vec<Repo> {
    items
        .iter()
        .map(|r| Repo {
            full_name: str_field(r, "full_name"),
            ssh_url: clone_link(r, "ssh"),
            https_url: clone_link(r, "https"),
            // Bitbucket reports bytes.
            size_kb: opt_u64_field(r, "size").map(|b| b / 1024),
        })
        .filter(|r| !r.full_name.is_empty())
        .collect()
}

/// Data Center has no `full_name`: it is built from the project key and the
/// repo slug, lowercased because keys are uppercase by convention and the name
/// doubles as the local directory.
fn parse_server(items: &[Value]) -> Vec<Repo> {
    items
        .iter()
        .filter_map(|r| {
            let key = r.pointer("/project/key").and_then(Value::as_str)?;
            let slug = r.get("slug").and_then(Value::as_str)?;
            (!key.is_empty() && !slug.is_empty()).then(|| Repo {
                full_name: format!("{}/{}", key.to_lowercase(), slug.to_lowercase()),
                ssh_url: clone_link(r, "ssh"),
                https_url: clone_link(r, "http"),
                ..Default::default()
            })
        })
        .collect()
}

fn cli_command() -> Command {
    let mut cmd = Command::new("bitbucket");
    cmd.args(["repo", "list", "--output", "json", "--limit", "1000"]);
    cmd
}

pub fn list_repos(host: &str) -> Result<Listing, ListError> {
    if host == CLOUD_HOST && has_cli("bitbucket") {
        let raw = run_capture(&mut cli_command())?;
        let items: Vec<Value> = serde_json::from_str(&raw).map_err(anyhow::Error::from)?;
        return Ok(parse(&items).into());
    }

    let Some(creds) = creds_for_host(host) else {
        return Err(ListError::Skip(format!(
            "no credentials for '{host}'. Run: repos-manager bitbucket login"
        )));
    };
    if host == CLOUD_HOST {
        // Listed by role: the stored user is an Atlassian email, not a workspace slug.
        let first =
            format!("https://api.bitbucket.org/2.0/repositories?role=member&pagelen={PAGE_SIZE}");
        Ok(list_cloud(&first, &creds)?)
    } else {
        Ok(list_server(&format!("https://{host}"), &creds)?)
    }
}

fn get_json(agent: &Agent, url: &str, auth: &str, what: &str) -> Result<Value> {
    Ok(agent
        .get(url)
        .header("Authorization", auth)
        .header("Accept", "application/json")
        .call()
        .with_context(|| what.to_string())?
        .body_mut()
        .read_json()?)
}

/// Follow the `next` links of the paginated 2.0 API from `first_page`.
fn list_cloud(first_page: &str, creds: &Creds) -> Result<Listing> {
    let agent = http::agent(Duration::from_secs(30));
    let auth = auth_header(creds);
    let mut next = Some(first_page.to_string());
    let mut items = Vec::new();
    for _ in 0..MAX_PAGES {
        let Some(url) = next.take() else { break };
        let page = get_json(&agent, &url, &auth, "Bitbucket API")?;
        if let Some(values) = page.get("values").and_then(Value::as_array) {
            items.extend(values.iter().cloned());
        }
        // A page pointing at itself would loop forever.
        next = page
            .get("next")
            .and_then(Value::as_str)
            .filter(|n| *n != url)
            .map(String::from);
    }
    // A page still pending means the cap cut the listing short.
    Ok(Listing {
        repos: parse(&items),
        partial: next.is_some(),
    })
}

/// The 1.0 API pages with `start`, reported back as `nextPageStart` until
/// `isLastPage`. Identifiers of adjacent objects are not contiguous, so the
/// offset must come from the payload and not from the number of items seen.
fn list_server(base: &str, creds: &Creds) -> Result<Listing> {
    let agent = http::agent(Duration::from_secs(30));
    let auth = auth_header(creds);
    let page_url = |start: Option<u64>| match start {
        Some(s) => format!("{base}/rest/api/1.0/repos?limit={PAGE_SIZE}&start={s}"),
        None => format!("{base}/rest/api/1.0/repos?limit={PAGE_SIZE}"),
    };
    let mut next = Some(page_url(None));
    let mut items = Vec::new();
    let mut seen = 0;
    for _ in 0..MAX_PAGES {
        let Some(url) = next.take() else { break };
        let page = get_json(&agent, &url, &auth, "Bitbucket Data Center API")?;
        if let Some(values) = page.get("values").and_then(Value::as_array) {
            items.extend(values.iter().cloned());
        }
        // An offset that does not move means the server ignores `start`.
        next = next_page_start(&page)
            .filter(|s| *s > seen)
            .inspect(|s| seen = *s)
            .map(|s| page_url(Some(s)));
    }
    Ok(Listing {
        repos: parse_server(&items),
        partial: next.is_some(),
    })
}

fn next_page_start(page: &Value) -> Option<u64> {
    if page.get("isLastPage").and_then(Value::as_bool) != Some(false) {
        return None;
    }
    page.get("nextPageStart").and_then(Value::as_u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::{Response, Server};
    use std::sync::{Arc, OnceLock};

    const CREDS_JSON: &str = r#"{"bb.corp.example":{"token":"dc"},
        "bitbucket.org":{"user":"me@example.com","token":"cloud"}}"#;

    fn cloud(user: &str, token: &str) -> Creds {
        Creds {
            user: Some(user.to_string()),
            token: token.to_string(),
        }
    }

    fn bearer(token: &str) -> Creds {
        Creds {
            user: None,
            token: token.to_string(),
        }
    }

    #[test]
    fn cli_lists_repos_as_json() {
        let cmd = cli_command();
        assert_eq!(cmd.get_program(), "bitbucket");
        assert_eq!(
            super::super::tests::args(&cmd),
            ["repo", "list", "--output", "json", "--limit", "1000"]
        );
    }

    #[test]
    fn parses_clone_links() {
        let items: Vec<Value> = serde_json::from_str(
            r#"[{"full_name":"team/repo","links":{"clone":[
                {"name":"https","href":"https://bitbucket.org/team/repo.git"},
                {"name":"ssh","href":"git@bitbucket.org:team/repo.git"}]}}]"#,
        )
        .unwrap();
        let r = &parse(&items)[0];
        assert_eq!(r.ssh_url, "git@bitbucket.org:team/repo.git");
        assert_eq!(r.https_url, "https://bitbucket.org/team/repo.git");
    }

    #[test]
    fn missing_links_and_names() {
        let items: Vec<Value> =
            serde_json::from_str(r#"[{"full_name":"t/r"},{"links":{}}]"#).unwrap();
        let repos = parse(&items);
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].ssh_url, "");
    }

    #[test]
    fn creds_json_round_trip() {
        let (map, legacy) = parse_creds(CREDS_JSON);
        assert!(!legacy);
        assert_eq!(map["bitbucket.org"].user.as_deref(), Some("me@example.com"));
        assert_eq!(map["bb.corp.example"].user, None);
        assert_eq!(map["bb.corp.example"].token, "dc");

        let merged = merged_creds(CREDS_JSON, "bb.corp.example", bearer("rotated")).unwrap();
        let (map, legacy) = parse_creds(&merged);
        assert!(!legacy);
        assert_eq!(map.len(), 2);
        assert_eq!(map["bb.corp.example"].token, "rotated");
        assert_eq!(map["bitbucket.org"], cloud("me@example.com", "cloud"));
        // A user-less entry must not serialize a null user.
        assert!(!merged.contains("null"), "{merged}");

        let fresh = merged_creds("", "bitbucket.org", cloud("a@b.c", "t")).unwrap();
        assert_eq!(parse_creds(&fresh).0.len(), 1);

        // Logging in to another host must not silence the legacy warning by
        // promoting a dead app password to a normal entry.
        let migrated = merged_creds("me:app-pass", "bb.corp.example", bearer("dc")).unwrap();
        let (map, legacy) = parse_creds(&migrated);
        assert!(!legacy);
        assert_eq!(map.len(), 1);
        assert_eq!(map["bb.corp.example"], bearer("dc"));
    }

    #[test]
    fn legacy_line_is_read_as_cloud() {
        let (map, legacy) = parse_creds("me:app-pass\n");
        assert!(legacy);
        assert_eq!(map["bitbucket.org"], cloud("me", "app-pass"));

        for bad in ["", "{}", "no-colon", ":only-token", "only-user:"] {
            let (map, legacy) = parse_creds(bad);
            assert!(map.is_empty() && !legacy, "{bad}");
        }
    }

    #[test]
    fn auth_header_basic_or_bearer() {
        assert_eq!(
            auth_header(&cloud("me@example.com", "tok")),
            format!("Basic {}", STANDARD.encode("me@example.com:tok"))
        );
        assert_eq!(auth_header(&bearer("tok")), "Bearer tok");
        // An empty user is as good as none.
        assert_eq!(auth_header(&cloud("", "tok")), "Bearer tok");
    }

    #[test]
    fn cloud_follows_next_links() {
        let base = Arc::new(OnceLock::<String>::new());
        let server = Server::start({
            let base = Arc::clone(&base);
            move |target| {
                if target.contains("page=2") {
                    Response::json(r#"{"values":[{"full_name":"t/b"}]}"#)
                } else {
                    let next = format!("{}/repositories?page=2", base.get().unwrap());
                    Response::json(&format!(
                        r#"{{"values":[{{"full_name":"t/a"}}],"next":"{next}"}}"#
                    ))
                }
            }
        });
        base.set(server.url.clone()).unwrap();

        let first = format!("{}/repositories?role=member&pagelen=100", server.url);
        let listing = list_cloud(&first, &cloud("user", "pass")).unwrap();
        let names: Vec<_> = listing.repos.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(names, ["t/a", "t/b"]);
        assert!(!listing.partial, "the last page ends the listing");
        let auth = format!("Basic {}", STANDARD.encode("user:pass"));
        let reqs = server.requests();
        assert_eq!(reqs.len(), 2);
        assert!(reqs.iter().all(|r| r.contains(&auth)));
    }

    #[test]
    fn parses_data_center_payload() {
        let items: Vec<Value> = serde_json::from_str(
            r#"[{"slug":"My-Repo","project":{"key":"PRJ"},"links":{"clone":[
                {"name":"http","href":"https://bb/scm/prj/my-repo.git"},
                {"name":"ssh","href":"ssh://git@bb:7999/prj/my-repo.git"}]}},
                {"slug":"orphan"},{"project":{"key":"PRJ"}},
                {"slug":"","project":{"key":"PRJ"}}]"#,
        )
        .unwrap();
        let repos = parse_server(&items);
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].full_name, "prj/my-repo");
        assert_eq!(repos[0].https_url, "https://bb/scm/prj/my-repo.git");
        assert_eq!(repos[0].ssh_url, "ssh://git@bb:7999/prj/my-repo.git");
        assert_eq!(repos[0].size_kb, None);
    }

    #[test]
    fn cloud_size_is_none_when_absent_and_zero_when_empty() {
        let items: Vec<Value> = serde_json::from_str(
            r#"[{"full_name":"t/none"},{"full_name":"t/empty","size":0},{"full_name":"t/big","size":2048}]"#,
        )
        .unwrap();
        let sizes: Vec<_> = parse(&items).into_iter().map(|r| r.size_kb).collect();
        assert_eq!(sizes, [None, Some(0), Some(2)]);
    }

    #[test]
    fn data_center_pages_over_next_page_start() {
        let server = Server::start(|target| {
            if target.contains("start=7") {
                Response::json(
                    r#"{"isLastPage":true,"values":[{"slug":"b","project":{"key":"PRJ"}}]}"#,
                )
            } else {
                Response::json(
                    r#"{"isLastPage":false,"nextPageStart":7,
                        "values":[{"slug":"a","project":{"key":"prj"}}]}"#,
                )
            }
        });

        let listing = list_server(&server.url, &cloud("me", "t")).unwrap();
        let names: Vec<_> = listing.repos.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(names, ["prj/a", "prj/b"]);
        assert!(!listing.partial);

        let reqs = server.requests();
        assert_eq!(reqs.len(), 2);
        assert!(
            reqs[0].contains("/rest/api/1.0/repos?limit=100 "),
            "{:?}",
            reqs[0]
        );
        assert!(reqs[1].contains("start=7"));
    }

    #[test]
    fn data_center_sends_bearer_without_user() {
        let server = Server::start(|_| Response::json(r#"{"isLastPage":true,"values":[]}"#));
        assert!(
            list_server(&server.url, &bearer("tok"))
                .unwrap()
                .repos
                .is_empty()
        );
        let reqs = server.requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].contains("Bearer tok"), "{:?}", reqs[0]);
        assert!(!reqs[0].contains("Basic"));
    }

    #[test]
    fn last_page_without_flags_stops() {
        let page: Value = serde_json::from_str(r#"{"isLastPage":false}"#).unwrap();
        assert_eq!(next_page_start(&page), None);
        let page: Value = serde_json::from_str(r#"{"nextPageStart":5}"#).unwrap();
        assert_eq!(next_page_start(&page), None);
    }

    #[test]
    fn api_errors_are_reported() {
        let server = Server::start(|_| Response::not_found());
        assert!(list_cloud(&server.url, &cloud("u", "p")).is_err());
        assert!(list_server(&server.url, &bearer("t")).is_err());
    }

    // Example response of GET /rest/api/1.0/repos in https://docs.atlassian.com/bitbucket-server/rest/7.7.0/bitbucket-rest.html
    const DC_REPOS_PAGE: &str = r#"{"size":1,"limit":25,"isLastPage":true,"values":[{
        "slug":"my-repo","id":1,"name":"My repo","description":"My repo description",
        "hierarchyId":"e3c939f9ef4a7fae272e","scmId":"git","state":"AVAILABLE",
        "statusMessage":"Available","forkable":true,
        "project":{"key":"PRJ","id":1,"name":"My Cool Project",
            "description":"The description for my cool project.","public":true,
            "type":"NORMAL","links":{"self":[{"href":"http://link/to/project"}]}},
        "public":true,
        "links":{"clone":[{"href":"ssh://git@<baseURL>/PRJ/my-repo.git","name":"ssh"},
                          {"href":"https://<baseURL>/scm/PRJ/my-repo.git","name":"http"}],
                 "self":[{"href":"http://link/to/repository"}]}}],"start":0}"#;

    #[test]
    fn documented_data_center_repository_is_parsed() {
        let page: Value = serde_json::from_str(DC_REPOS_PAGE).unwrap();
        let values = page["values"].as_array().unwrap();
        let repos = parse_server(values);
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].full_name, "prj/my-repo");
        assert_eq!(repos[0].https_url, "https://<baseURL>/scm/PRJ/my-repo.git");
        assert_eq!(repos[0].ssh_url, "ssh://git@<baseURL>/PRJ/my-repo.git");
    }

    #[test]
    fn documented_data_center_pages_follow_the_paging_fields() {
        // The paging envelope is the one of the "Paged APIs" section of the same document.
        let first = DC_REPOS_PAGE.replace(
            r#""isLastPage":true"#,
            r#""isLastPage":false,"filter":null,"nextPageStart":25"#,
        );
        let server = Server::start(move |target| {
            if target.contains("start=25") {
                Response::json(DC_REPOS_PAGE)
            } else {
                Response::json(&first)
            }
        });
        let listing = list_server(&server.url, &bearer("t")).unwrap();
        assert_eq!(listing.repos.len(), 2);
        assert!(!listing.partial);
        assert!(server.requests()[1].contains("start=25"));
    }
}
