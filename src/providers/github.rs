use std::process::Command;

use serde_json::Value;

use super::{
    ListError, Repo, parse_paginated, require_cli, run_capture, str_field, with_git_suffix,
};

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
    require_cli("gh")?;
    // `gh repo list` only returns the user's own repos; /user/repos with the full
    // affiliation set also covers collaborations on other personal accounts.
    let mut cmd = Command::new("gh");
    cmd.args([
        "api",
        "--paginate",
        "/user/repos?affiliation=owner,collaborator,organization_member&per_page=100",
    ]);
    if host != "github.com" {
        cmd.args(["--hostname", host]);
    }
    let raw = run_capture(&mut cmd)?;
    Ok(parse(&parse_paginated(&raw)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_api_payload() {
        let items: Vec<Value> = serde_json::from_str(
            r#"[{"full_name":"Dxsk/repo","ssh_url":"git@github.com:Dxsk/repo.git","html_url":"https://github.com/Dxsk/repo"}]"#,
        )
        .unwrap();
        let r = &parse(&items)[0];
        assert_eq!(r.full_name, "Dxsk/repo");
        assert_eq!(r.ssh_url, "git@github.com:Dxsk/repo.git");
        assert_eq!(r.https_url, "https://github.com/Dxsk/repo.git");
    }
}
