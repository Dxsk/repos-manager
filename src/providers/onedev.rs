//! OneDev: no official CLI, so the REST API is called with an access token
//! saved by `repos-manager onedev login`.

use std::path::PathBuf;

use anyhow::Result;

use super::{ListError, Repo};
use crate::config;

fn creds_path() -> PathBuf {
    config::app_config_dir().join("onedev-creds")
}

pub fn has_api_creds() -> bool {
    creds_path().is_file()
}

pub fn login() -> Result<()> {
    Ok(())
}

pub fn list_repos(_host: &str) -> Result<Vec<Repo>, ListError> {
    Err(ListError::Skip("not implemented yet".into()))
}
