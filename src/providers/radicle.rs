use std::process::Command;

use anyhow::{Result, bail};
use serde_json::Value;

use super::{
    ListError, Listing, Repo, has_cli, require_cli, run_capture, run_interactive, str_field,
};

pub fn login() -> Result<()> {
    if !has_cli("rad") {
        bail!("rad CLI not found. Install it from https://radicle.xyz");
    }
    run_interactive("rad", &["auth"])
}

pub fn parse(items: &[Value]) -> Vec<Repo> {
    items
        .iter()
        .map(|r| {
            let url = format!("rad://{}", str_field(r, "id"));
            Repo {
                full_name: format!("{}/{}", str_field(r, "namespace"), str_field(r, "name")),
                ssh_url: url.clone(),
                https_url: url,
                ..Default::default()
            }
        })
        .collect()
}

fn command() -> Command {
    let mut cmd = Command::new("rad");
    cmd.args(["ls", "--json"]);
    cmd
}

pub fn list_repos() -> Result<Listing, ListError> {
    require_cli("rad")?;
    let raw = run_capture(&mut command())?;
    let items: Vec<Value> = serde_json::from_str(&raw).map_err(anyhow::Error::from)?;
    Ok(parse(&items).into())
}

#[cfg(test)]
mod tests {
    use super::super::tests::args;
    use super::*;

    #[test]
    fn lists_repos_as_json() {
        let cmd = command();
        assert_eq!(cmd.get_program(), "rad");
        assert_eq!(args(&cmd), ["ls", "--json"]);
    }

    #[test]
    fn parses_rad_ls() {
        let items: Vec<Value> = serde_json::from_str(
            r#"[{"id":"z3abc","namespace":"did:key:z6Mk","name":"heartwood"}]"#,
        )
        .unwrap();
        let r = &parse(&items)[0];
        assert_eq!(r.full_name, "did:key:z6Mk/heartwood");
        assert_eq!(r.ssh_url, "rad://z3abc");
        assert_eq!(r.clone_url(true), "rad://z3abc");
    }
}
