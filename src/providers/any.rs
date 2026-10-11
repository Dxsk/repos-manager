//! A plain git server, with no forge API: repository URLs come from the
//! config, or from the servers that can list them over SSH.

use anyhow::Result;

use super::{ListError, Repo};
use crate::config::Settings;
use crate::output;

pub fn login() -> Result<()> {
    output::info("any: nothing to authenticate, these hosts are reached with your SSH keys.");
    Ok(())
}

pub fn list_repos(host: &str, settings: &Settings) -> Result<Vec<Repo>, ListError> {
    let Some(spec) = settings.any_host(host) else {
        return Err(ListError::Skip(format!(
            "no repos configured for '{host}'. Add them under \"repos\" in the config"
        )));
    };
    let _ = spec;
    Err(ListError::Skip("not implemented yet".into()))
}
