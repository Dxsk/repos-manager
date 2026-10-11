use std::process::Command;

use serde_json::Value;

use super::{
    ListError, Listing, Repo, opt_u64_field, parse_paginated, require_cli, run_capture, str_field,
    with_git_suffix,
};

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

fn command(host: &str) -> Command {
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
    cmd
}

pub fn list_repos(host: &str) -> Result<Listing, ListError> {
    require_cli("gh")?;
    let raw = run_capture(&mut command(host))?;
    Ok(parse(&parse_paginated(&raw)?).into())
}

#[cfg(test)]
mod tests {
    use super::super::tests::{args, envs};
    use super::*;

    #[test]
    fn targets_enterprise_hosts_only_when_needed() {
        let public = args(&command("github.com"));
        assert_eq!(public[0], "api");
        assert!(public.contains(&"--paginate".to_string()));
        assert!(!public.contains(&"--hostname".to_string()));

        let ghe = args(&command("ghe.corp.example"));
        assert_eq!(ghe[ghe.len() - 2..], ["--hostname", "ghe.corp.example"]);
        assert!(envs(&command("ghe.corp.example")).is_empty());
    }

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
        assert_eq!(r.size_kb, None);
    }

    #[test]
    fn empty_repo_keeps_its_zero_size() {
        let items: Vec<Value> = serde_json::from_str(
            r#"[{"full_name":"o/empty","size":0},{"full_name":"o/big","size":42}]"#,
        )
        .unwrap();
        let sizes: Vec<_> = parse(&items).into_iter().map(|r| r.size_kb).collect();
        assert_eq!(sizes, [Some(0), Some(42)]);
    }

    #[test]
    fn drops_entries_without_name() {
        let items: Vec<Value> =
            serde_json::from_str(r#"[{"ssh_url":"x"},{"full_name":"o/r","html_url":""}]"#).unwrap();
        let repos = parse(&items);
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].https_url, "");
    }
}
