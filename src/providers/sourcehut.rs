//! Sourcehut, through the official `hut` CLI.

use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::{
    ListError, Listing, Repo, has_cli, require_cli, run_capture_input, run_interactive, str_field,
};

/// The git.sr.ht root `repositories` field returns every repo the token can
/// reach, private and unlisted included, so nothing is filtered here.
const QUERY: &str = "query($cursor: Cursor) { repositories(cursor: $cursor) \
    { cursor results { name owner { canonicalName } } } }";

/// A page holds up to a few dozen repos: a cap keeps a server that keeps
/// handing back the same cursor from looping forever.
const MAX_PAGES: usize = 200;

const NOT_CONFIGURED: [&str; 7] = [
    "hasn't been set up",
    "missing access-token",
    "no token provided",
    "no sr.ht instance configured",
    "no instance named",
    "unauthorized",
    "401",
];

pub fn login() -> Result<()> {
    if !has_cli("hut") {
        bail!("hut CLI not found. Install it: https://sr.ht/~xenrox/hut");
    }
    run_interactive("hut", &["init"])
}

/// `--instance` takes the host as it is: hut matches an instance by subdomain
/// too, so `git.sr.ht` resolves the `sr.ht` entry of its config.
pub fn command(host: &str, cursor: Option<&str>) -> Command {
    let mut cmd = Command::new("hut");
    cmd.args(["graphql", "git", "--stdin", "--instance", host]);
    if let Some(cursor) = cursor {
        cmd.args(["-v", &format!("cursor={cursor}")]);
    }
    cmd
}

pub fn parse(host: &str, items: &[Value]) -> Vec<Repo> {
    items.iter().filter_map(|item| repo(host, item)).collect()
}

/// Sourcehut names keep their `~`: it is part of the clone URL and of the
/// local path. sr.ht serves repos without a `.git` suffix.
fn repo(host: &str, item: &Value) -> Option<Repo> {
    let name = str_field(item, "name");
    let owner = item.get("owner").map(|o| str_field(o, "canonicalName"))?;
    if name.is_empty() || owner.is_empty() {
        return None;
    }
    let full_name = format!("{owner}/{name}");
    Some(Repo {
        ssh_url: format!("git@{host}:{full_name}"),
        https_url: format!("https://{host}/{full_name}"),
        full_name,
        ..Default::default()
    })
}

/// `hut` prints what gqlclient decoded out of the response's `data` field, so
/// the payload starts at `repositories`. An envelope is unwrapped anyway: a
/// shape change there would otherwise list zero repos without a word.
fn payload(page: &Value) -> &Value {
    match page.get("data") {
        Some(inner) if inner.is_object() => inner,
        _ => page,
    }
}

fn results(page: &Value) -> &[Value] {
    payload(page)
        .pointer("/repositories/results")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn next_cursor(page: &Value) -> Option<String> {
    payload(page)
        .pointer("/repositories/cursor")
        .and_then(Value::as_str)
        .filter(|c| !c.is_empty())
        .map(str::to_string)
}

/// hut exits the same way whether it has no token or the API refused the
/// query, so only its message tells a skippable host from a real failure.
fn classify(err: anyhow::Error) -> ListError {
    let msg = err.to_string().to_lowercase();
    if NOT_CONFIGURED.iter().any(|m| msg.contains(m)) {
        ListError::Skip("hut has no token for this instance. Run `hut init`".into())
    } else {
        ListError::Fail(err)
    }
}

/// `hut graphql` reads its query from stdin only, hence the stdin variant of
/// the capture helper.
fn fetch_page(host: &str, cursor: Option<&str>) -> Result<Value, ListError> {
    let raw = run_capture_input(&mut command(host, cursor), QUERY).map_err(classify)?;
    Ok(serde_json::from_str(&raw).context("invalid JSON from hut")?)
}

pub fn list_repos(host: &str) -> Result<Listing, ListError> {
    require_cli("hut")?;
    let mut repos = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_PAGES {
        let page = fetch_page(host, cursor.as_deref())?;
        repos.extend(parse(host, results(&page)));
        cursor = next_cursor(&page);
        if cursor.is_none() {
            break;
        }
    }
    // A cursor still in hand means the cap cut the listing short.
    Ok(Listing {
        repos,
        partial: cursor.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    const PAGE: &str = r#"{"repositories":{"cursor":"MjAyNS0xMC0xMVQw",
        "results":[
          {"name":"repos-manager","owner":{"canonicalName":"~dxsk"}},
          {"name":"secret","owner":{"canonicalName":"~dxsk"}},
          {"name":"shared","owner":{"canonicalName":"~emersion"}}
        ]}}"#;

    #[test]
    fn parses_a_page() {
        let page: Value = serde_json::from_str(PAGE).unwrap();
        let repos = parse("git.sr.ht", results(&page));
        assert_eq!(repos.len(), 3);
        assert_eq!(repos[0].full_name, "~dxsk/repos-manager");
        assert_eq!(repos[0].ssh_url, "git@git.sr.ht:~dxsk/repos-manager");
        assert_eq!(repos[0].https_url, "https://git.sr.ht/~dxsk/repos-manager");
        // A repo shared through an ACL keeps its own owner.
        assert_eq!(repos[2].full_name, "~emersion/shared");
        assert_eq!(next_cursor(&page).as_deref(), Some("MjAyNS0xMC0xMVQw"));
    }

    #[test]
    fn urls_follow_the_host() {
        let page: Value = serde_json::from_str(PAGE).unwrap();
        let repos = parse("git.example.org", results(&page));
        assert_eq!(repos[0].ssh_url, "git@git.example.org:~dxsk/repos-manager");
        assert_eq!(
            repos[0].https_url,
            "https://git.example.org/~dxsk/repos-manager"
        );
    }

    #[test]
    fn a_data_envelope_is_unwrapped() {
        let raw = format!(r#"{{"data":{}}}"#, PAGE);
        let page: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parse("git.sr.ht", results(&page)).len(), 3);
        assert_eq!(next_cursor(&page).as_deref(), Some("MjAyNS0xMC0xMVQw"));
    }

    #[test]
    fn last_page_has_no_cursor() {
        let page: Value =
            serde_json::from_str(r#"{"repositories":{"cursor":null,"results":[]}}"#).unwrap();
        assert!(next_cursor(&page).is_none());
        assert!(results(&page).is_empty());
        let empty: Value = serde_json::from_str("{}").unwrap();
        assert!(next_cursor(&empty).is_none());
        assert!(results(&empty).is_empty());
    }

    #[test]
    fn drops_entries_without_a_name_or_owner() {
        let items: Vec<Value> = serde_json::from_str(
            r#"[{"owner":{"canonicalName":"~dxsk"}},{"name":"r"},{"name":"r","owner":{}}]"#,
        )
        .unwrap();
        assert!(parse("git.sr.ht", &items).is_empty());
    }

    #[test]
    fn builds_the_query_command() {
        let cmd = command("git.sr.ht", None);
        assert_eq!(cmd.get_program(), "hut");
        assert_eq!(
            super::super::tests::args(&cmd),
            ["graphql", "git", "--stdin", "--instance", "git.sr.ht"]
        );

        let cmd = command("git.example.org", Some("MjA="));
        assert_eq!(
            super::super::tests::args(&cmd),
            [
                "graphql",
                "git",
                "--stdin",
                "--instance",
                "git.example.org",
                "-v",
                "cursor=MjA="
            ]
        );
    }

    #[test]
    fn missing_token_skips_the_host() {
        for stderr in [
            "Looks like hut's config file hasn't been set up yet",
            "instance \"sr.ht\": missing access-token or access-token-cmd",
            "no instance named \"git.example.org\" found",
            "GraphQL: 401 Unauthorized",
        ] {
            let ListError::Skip(msg) = classify(anyhow!("hut failed: {stderr}")) else {
                panic!("expected a skip for {stderr}");
            };
            assert!(msg.contains("hut init"), "{msg}");
        }
    }

    #[test]
    fn api_errors_fail_the_host() {
        let err = classify(anyhow!("hut failed: GraphQL: internal server error"));
        assert!(matches!(err, ListError::Fail(e) if e.to_string().contains("internal")));
    }

    #[test]
    fn the_query_asks_for_what_the_parser_reads() {
        for field in ["repositories", "cursor", "results", "name", "canonicalName"] {
            assert!(QUERY.contains(field), "{field} missing from the query");
        }
    }

    // Shape from the Repository and RepositoryCursor types of https://docs.sourcehut.org/git.sr.ht/ (values are illustrative).
    const GRAPHQL_PAGE: &str = r##"{"repositories":{"cursor":"MjU=","results":[
        {"id":1,"rid":"b9d5bf1a-0c58-4b3d-9f06-6a3c0a1e2f11",
         "created":"2022-03-14T09:26:53Z","updated":"2024-11-02T17:04:10Z",
         "owner":{"canonicalName":"~dxsk"},"name":"repos-manager",
         "description":"Sync every repository you can access","visibility":"PUBLIC",
         "readme":null,"access":"RW"},
        {"id":2,"rid":"0f1c6b52-7e0d-4a7e-8d5f-1b2a3c4d5e6f",
         "created":"2023-01-05T12:00:00Z","updated":"2023-01-05T12:00:00Z",
         "owner":{"canonicalName":"~dxsk"},"name":"dotfiles",
         "description":null,"visibility":"PRIVATE","readme":"# dotfiles","access":"RW"}
    ]}}"##;

    #[test]
    fn full_graphql_page_lists_repos_and_cursor() {
        let page: Value = serde_json::from_str(GRAPHQL_PAGE).unwrap();
        let repos = parse("git.sr.ht", results(&page));
        let names: Vec<_> = repos.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(names, ["~dxsk/repos-manager", "~dxsk/dotfiles"]);
        assert_eq!(repos[0].https_url, "https://git.sr.ht/~dxsk/repos-manager");
        assert_eq!(repos[0].ssh_url, "git@git.sr.ht:~dxsk/repos-manager");
        assert_eq!(next_cursor(&page).as_deref(), Some("MjU="));
    }

    #[test]
    fn full_graphql_page_inside_a_data_envelope() {
        let wrapped = format!(r#"{{"data":{GRAPHQL_PAGE}}}"#);
        let page: Value = serde_json::from_str(&wrapped).unwrap();
        assert_eq!(results(&page).len(), 2);
        assert_eq!(next_cursor(&page).as_deref(), Some("MjU="));
    }

    #[test]
    fn last_graphql_page_has_a_null_cursor() {
        let page: Value =
            serde_json::from_str(r#"{"repositories":{"cursor":null,"results":[]}}"#).unwrap();
        assert!(results(&page).is_empty());
        assert_eq!(next_cursor(&page), None);
    }
}
