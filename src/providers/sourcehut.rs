//! Sourcehut, through the official `hut` CLI.

use anyhow::{Result, bail};

use super::{ListError, Repo, has_cli, run_interactive};

pub fn login() -> Result<()> {
    if !has_cli("hut") {
        bail!("hut CLI not found. Install it: https://sr.ht/~xenrox/hut");
    }
    run_interactive("hut", &["init"])
}

pub fn list_repos(_host: &str) -> Result<Vec<Repo>, ListError> {
    Err(ListError::Skip("not implemented yet".into()))
}
