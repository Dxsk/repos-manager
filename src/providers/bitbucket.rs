//! Bitbucket: uses the `bitbucket` CLI when installed, otherwise the REST
//! API with an app password saved by `repos-manager bitbucket login`.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;

use super::{ListError, Repo, has_cli, run_capture, run_interactive, str_field};
use crate::{config, http, output};

fn creds_path() -> PathBuf {
    config::app_config_dir().join("bitbucket-creds")
}

pub fn has_api_creds() -> bool {
    creds_path().is_file()
}

pub fn login() -> Result<()> {
    if has_cli("bitbucket") {
        return run_interactive("bitbucket", &["auth", "login"]);
    }
    output::info("No bitbucket CLI found. Using API app password auth.");
    output::info("Create one at: https://bitbucket.org/account/settings/app-passwords/");
    print!("Bitbucket username: ");
    io::stdout().flush()?;
    let mut user = String::new();
    io::stdin().read_line(&mut user)?;
    let pass = rpassword::prompt_password("App password: ")?;

    let path = creds_path();
    fs::create_dir_all(path.parent().expect("creds path has a parent"))?;
    fs::write(&path, format!("{}:{}", user.trim(), pass.trim()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    output::success(&format!("Credentials saved to {}", path.display()));
    Ok(())
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
        })
        .filter(|r| !r.full_name.is_empty())
        .collect()
}

pub fn list_repos() -> Result<Vec<Repo>, ListError> {
    if has_cli("bitbucket") {
        let raw = run_capture(
            Command::new("bitbucket").args(["repo", "list", "--output", "json", "--limit", "1000"]),
        )?;
        let items: Vec<Value> = serde_json::from_str(&raw).map_err(anyhow::Error::from)?;
        return Ok(parse(&items));
    }

    let Ok(creds) = fs::read_to_string(creds_path()) else {
        return Err(ListError::Skip(
            "not authenticated. Run: repos-manager bitbucket login".into(),
        ));
    };
    let creds = creds.trim();
    let user = creds.split(':').next().unwrap_or_default();
    Ok(list_from_api(
        &format!("https://api.bitbucket.org/2.0/repositories/{user}?pagelen=100"),
        creds,
    )?)
}

/// Follow the `next` links of the paginated API from `first_page`.
fn list_from_api(first_page: &str, creds: &str) -> Result<Vec<Repo>> {
    let auth = format!("Basic {}", STANDARD.encode(creds));
    let agent = http::agent(Duration::from_secs(30));
    let mut next = Some(first_page.to_string());
    let mut items = Vec::new();
    while let Some(url) = next {
        let page: Value = agent
            .get(&url)
            .header("Authorization", &auth)
            .call()
            .context("Bitbucket API")?
            .body_mut()
            .read_json()?;
        if let Some(values) = page.get("values").and_then(Value::as_array) {
            items.extend(values.iter().cloned());
        }
        next = page.get("next").and_then(Value::as_str).map(String::from);
    }
    Ok(parse(&items))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::{Response, Server};
    use std::sync::{Arc, OnceLock};

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
    fn api_follows_next_links() {
        let base = Arc::new(OnceLock::<String>::new());
        let server = Server::start({
            let base = Arc::clone(&base);
            move |target| {
                if target.contains("page=2") {
                    Response::json(r#"{"values":[{"full_name":"t/b"}]}"#)
                } else {
                    let next = format!("{}/repositories/t?page=2", base.get().unwrap());
                    Response::json(&format!(
                        r#"{{"values":[{{"full_name":"t/a"}}],"next":"{next}"}}"#
                    ))
                }
            }
        });
        base.set(server.url.clone()).unwrap();

        let first = format!("{}/repositories/t?pagelen=100", server.url);
        let repos = list_from_api(&first, "user:pass").unwrap();
        let names: Vec<_> = repos.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(names, ["t/a", "t/b"]);
        let auth = format!("Basic {}", STANDARD.encode("user:pass"));
        let reqs = server.requests();
        assert_eq!(reqs.len(), 2);
        assert!(reqs.iter().all(|r| r.contains(&auth)));
    }

    #[test]
    fn api_error_is_reported() {
        let server = Server::start(|_| Response::not_found());
        assert!(list_from_api(&server.url, "u:p").is_err());
    }
}
