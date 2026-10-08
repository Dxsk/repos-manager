use std::process::Command;

use anyhow::{Result, bail};
use serde_json::Value;

use super::{ListError, Repo, has_cli, require_cli, run_capture, run_interactive, str_field};

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
            }
        })
        .collect()
}

pub fn list_repos() -> Result<Vec<Repo>, ListError> {
    require_cli("rad")?;
    let raw = run_capture(Command::new("rad").args(["ls", "--json"]))?;
    let items: Vec<Value> = serde_json::from_str(&raw).map_err(anyhow::Error::from)?;
    Ok(parse(&items))
}
