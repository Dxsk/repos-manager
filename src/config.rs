//! Settings resolution: CLI flag > `REPOS_MANAGER_*` env var > config.json > defaults.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::output;
use crate::providers::Provider;

pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Kept at `~/.config/repos-manager` on every OS so the documented path is the same everywhere.
pub fn app_config_dir() -> PathBuf {
    home_dir().join(".config").join("repos-manager")
}

pub fn config_path() -> PathBuf {
    std::env::var_os("REPOS_MANAGER_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| app_config_dir().join("config.json"))
}

/// Accepts both the legacy string form and the list form for `hosts.<provider>`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Hosts {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    base_dir: Option<String>,
    parallel: Option<usize>,
    protocol: Option<String>,
    check_updates: Option<bool>,
    scan_network_mounts: Option<bool>,
    #[serde(default)]
    hosts: HashMap<String, Hosts>,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub base_dir: PathBuf,
    pub parallel: usize,
    pub use_https: bool,
    pub check_updates: bool,
    pub scan_network_mounts: bool,
    hosts: HashMap<Provider, Vec<String>>,
}

fn expand_tilde(s: &str) -> PathBuf {
    match s.strip_prefix('~') {
        Some(rest) => home_dir().join(rest.trim_start_matches(['/', '\\'])),
        None => PathBuf::from(s),
    }
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

impl Settings {
    pub fn load() -> Result<Self> {
        Self::load_from(&config_path())
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let file: FileConfig = if path.is_file() {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            serde_json::from_str(&raw)
                .with_context(|| format!("invalid config file {}", path.display()))?
        } else {
            FileConfig::default()
        };

        let base_dir = env_nonempty("REPOS_MANAGER_BASE_DIR")
            .or(file.base_dir)
            .map(|s| expand_tilde(&s))
            .unwrap_or_else(|| home_dir().join("Documents"));

        let parallel = env_nonempty("REPOS_MANAGER_PARALLEL")
            .and_then(|v| v.parse().ok())
            .or(file.parallel)
            .unwrap_or(4);

        let protocol = env_nonempty("REPOS_MANAGER_PROTOCOL").or(file.protocol);
        let use_https = protocol.as_deref() == Some("https");

        let check_updates = env_nonempty("REPOS_MANAGER_NO_UPDATE_CHECK").as_deref() != Some("1")
            && file.check_updates.unwrap_or(true);

        let mut hosts: HashMap<Provider, Vec<String>> = Provider::ALL
            .iter()
            .map(|p| (*p, vec![p.default_host().to_string()]))
            .collect();
        for (key, value) in file.hosts {
            let Some(provider) = Provider::from_name(&key) else {
                output::warn(&format!("config: unknown provider in hosts: {key}"));
                continue;
            };
            let list = match value {
                Hosts::One(h) => vec![h],
                Hosts::Many(v) => v,
            };
            let list: Vec<String> = list.into_iter().filter(|h| !h.is_empty()).collect();
            if !list.is_empty() {
                hosts.insert(provider, list);
            }
        }

        Ok(Self {
            base_dir,
            parallel: parallel.max(1),
            use_https,
            check_updates,
            scan_network_mounts: file.scan_network_mounts.unwrap_or(false),
            hosts,
        })
    }

    pub fn hosts(&self, provider: Provider) -> &[String] {
        self.hosts.get(&provider).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn validate_base_dir(&self) -> Result<()> {
        if self.base_dir.as_os_str().is_empty() {
            bail!("base directory is empty");
        }
        if !self.base_dir.is_absolute() {
            bail!(
                "base directory must be an absolute path: {}",
                self.base_dir.display()
            );
        }
        fs::create_dir_all(&self.base_dir)
            .with_context(|| format!("cannot create base directory {}", self.base_dir.display()))
    }
}

const DEFAULT_CONFIG: &str = r#"{
  "base_dir": "~/Documents",
  "parallel": 4,
  "protocol": "ssh",
  "check_updates": true,
  "scan_network_mounts": false,
  "hosts": {
    "github":    ["github.com"],
    "gitlab":    ["gitlab.com"],
    "forgejo":   ["codeberg.org"],
    "bitbucket": ["bitbucket.org"]
  }
}
"#;

pub fn init_config() -> Result<()> {
    let path = config_path();
    if path.exists() {
        output::warn(&format!("Config already exists: {}", path.display()));
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
    }
    fs::write(&path, DEFAULT_CONFIG)?;
    output::success(&format!("Config created: {}", path.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        fs::write(&p, content).unwrap();
        (dir, p)
    }

    #[test]
    fn defaults_without_file() {
        let s = Settings::load_from(Path::new("/nonexistent/config.json")).unwrap();
        assert_eq!(s.hosts(Provider::Forgejo), ["codeberg.org"]);
        assert!(s.check_updates);
        assert!(!s.scan_network_mounts);
    }

    #[test]
    fn hosts_string_and_list_forms() {
        let (_d, p) = write(r#"{"hosts":{"gitlab":"gl.example.org","forgejo":["a.org","b.org"]}}"#);
        let s = Settings::load_from(&p).unwrap();
        assert_eq!(s.hosts(Provider::Gitlab), ["gl.example.org"]);
        assert_eq!(s.hosts(Provider::Forgejo), ["a.org", "b.org"]);
        assert_eq!(s.hosts(Provider::Github), ["github.com"]);
    }

    #[test]
    fn explicit_false_is_respected() {
        let (_d, p) =
            write(r#"{"check_updates":false,"scan_network_mounts":true,"protocol":"https"}"#);
        let s = Settings::load_from(&p).unwrap();
        assert!(!s.check_updates);
        assert!(s.scan_network_mounts);
        assert!(s.use_https);
    }

    #[test]
    fn tilde_expansion() {
        assert_eq!(expand_tilde("~/Documents"), home_dir().join("Documents"));
        assert_eq!(expand_tilde("/abs"), PathBuf::from("/abs"));
    }

    #[test]
    fn invalid_json_is_an_error() {
        let (_d, p) = write("{not json");
        assert!(Settings::load_from(&p).is_err());
    }
}
