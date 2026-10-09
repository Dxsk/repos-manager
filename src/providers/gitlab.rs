use std::process::Command;

use serde_json::Value;

use super::{ListError, Repo, parse_paginated, require_cli, run_capture, str_field};

pub fn parse(items: &[Value]) -> Vec<Repo> {
    items
        .iter()
        .map(|r| Repo {
            full_name: str_field(r, "path_with_namespace"),
            ssh_url: str_field(r, "ssh_url_to_repo"),
            https_url: str_field(r, "http_url_to_repo"),
        })
        .filter(|r| !r.full_name.is_empty())
        .collect()
}

fn command(host: &str) -> Command {
    let mut cmd = Command::new("glab");
    cmd.env("GITLAB_HOST", host).args([
        "api",
        "projects?membership=true&per_page=100&simple=true",
        "--paginate",
    ]);
    cmd
}

pub fn list_repos(host: &str) -> Result<Vec<Repo>, ListError> {
    require_cli("glab")?;
    let raw = run_capture(&mut command(host))?;
    Ok(parse(&parse_paginated(&raw)?))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{args, envs};
    use super::*;

    #[test]
    fn routes_every_host_through_gitlab_host() {
        for host in ["gitlab.com", "gl.example.org:8443"] {
            let cmd = command(host);
            assert_eq!(envs(&cmd), [("GITLAB_HOST".to_string(), host.to_string())]);
            assert_eq!(
                args(&cmd)[..2],
                ["api", "projects?membership=true&per_page=100&simple=true"]
            );
            assert!(args(&cmd).contains(&"--paginate".to_string()));
        }
    }

    #[test]
    fn parses_api_payload() {
        let items: Vec<Value> = serde_json::from_str(
            r#"[{"path_with_namespace":"group/sub/project","ssh_url_to_repo":"git@gitlab.com:group/sub/project.git","http_url_to_repo":"https://gitlab.com/group/sub/project.git"}]"#,
        )
        .unwrap();
        let r = &parse(&items)[0];
        assert_eq!(r.full_name, "group/sub/project");
        assert_eq!(r.https_url, "https://gitlab.com/group/sub/project.git");
    }

    #[test]
    fn drops_entries_without_path() {
        let items: Vec<Value> = serde_json::from_str(r#"[{"ssh_url_to_repo":"x"}]"#).unwrap();
        assert!(parse(&items).is_empty());
    }
}
