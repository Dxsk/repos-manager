//! Settings resolution: CLI flag > `REPOS_MANAGER_*` env var > config.json > defaults.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::output;
use crate::plan::{self, Confirm, ConfirmSync};
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

/// One entry of `repos`: either a bare list of clone URLs, or a host that can
/// list its own repositories over SSH.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ReposEntry {
    Urls(Vec<String>),
    Spec(AnyHost),
}

/// A plain git server reached over SSH.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AnyHost {
    /// Clone URLs listed by hand.
    #[serde(default)]
    pub urls: Vec<String>,
    /// How to ask the server for its repositories: `gitolite`, `soft-serve`
    /// or `find:<root>`. Without it, only `urls` are synced.
    pub discover: Option<String>,
    /// SSH port, when it is not 22 (Soft Serve defaults to 23231).
    pub port: Option<u16>,
    /// SSH user, when it is not the local one (`git` for gitolite).
    pub user: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    base_dir: Option<String>,
    parallel: Option<usize>,
    protocol: Option<String>,
    check_updates: Option<bool>,
    scan_network_mounts: Option<bool>,
    gitlab_statistics: Option<bool>,
    confirm_sync: Option<String>,
    confirm_new_repos: Option<usize>,
    confirm_download_gb: Option<f64>,
    #[serde(default)]
    hosts: HashMap<String, Hosts>,
    #[serde(default)]
    repos: BTreeMap<String, ReposEntry>,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub base_dir: PathBuf,
    pub parallel: usize,
    pub use_https: bool,
    pub check_updates: bool,
    pub scan_network_mounts: bool,
    pub gitlab_statistics: bool,
    /// When a sync stops to confirm what it is about to clone.
    pub confirm: Confirm,
    hosts: HashMap<Provider, Vec<String>>,
    any: BTreeMap<String, AnyHost>,
}

/// `confirm_sync` and its thresholds. These describe the machine, not the
/// workspace: the filters live in `.repos.yml`, next to the repositories.
fn confirm_settings(file: &FileConfig) -> Confirm {
    let mut confirm = Confirm::default();
    if let Some(mode) = &file.confirm_sync {
        match ConfirmSync::parse(mode) {
            Some(m) => confirm.mode = m,
            None => output::warn(&format!(
                "config: unknown confirm_sync value: {mode}, using auto"
            )),
        }
    }
    if let Some(n) = file.confirm_new_repos {
        confirm.new_repos = n;
    }
    if let Some(gb) = file.confirm_download_gb {
        confirm.download_kb = plan::gb_to_kb(gb);
    }
    confirm
}

fn expand_tilde(s: &str) -> PathBuf {
    match s.strip_prefix('~') {
        Some(rest) => home_dir().join(rest.trim_start_matches(['/', '\\'])),
        None => PathBuf::from(s),
    }
}

impl Settings {
    pub fn load() -> Result<Self> {
        Self::load_from(&config_path(), |key| std::env::var(key).ok())
    }

    /// `env` is injected so tests do not race on the process environment.
    pub fn load_from(path: &Path, env: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let env_nonempty = |key: &str| env(key).filter(|v| !v.is_empty());
        let file: FileConfig = if path.is_file() {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("cannot read {}", output::path(path)))?;
            serde_json::from_str(&raw)
                .with_context(|| format!("invalid config file {}", output::path(path)))?
        } else {
            FileConfig::default()
        };

        let confirm = confirm_settings(&file);
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

        let any: BTreeMap<String, AnyHost> = file
            .repos
            .into_iter()
            .map(|(host, entry)| {
                let spec = match entry {
                    ReposEntry::Urls(urls) => AnyHost {
                        urls,
                        ..Default::default()
                    },
                    ReposEntry::Spec(spec) => spec,
                };
                (host, spec)
            })
            .filter(|(host, spec)| {
                !host.is_empty() && (!spec.urls.is_empty() || spec.discover.is_some())
            })
            .collect();

        // Self-hosted-only providers have no default host; `any` takes its
        // hosts from the `repos` keys unless `hosts.any` says otherwise.
        let mut hosts: HashMap<Provider, Vec<String>> = Provider::ALL
            .iter()
            .filter(|p| !p.default_host().is_empty())
            .map(|p| (*p, vec![p.default_host().to_string()]))
            .collect();
        hosts.insert(Provider::Any, any.keys().cloned().collect());
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
            gitlab_statistics: file.gitlab_statistics.unwrap_or(true),
            confirm,
            hosts,
            any,
        })
    }

    pub fn hosts(&self, provider: Provider) -> &[String] {
        self.hosts.get(&provider).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The `repos` entry of a plain git host, if it has one.
    pub fn any_host(&self, host: &str) -> Option<&AnyHost> {
        self.any.get(host)
    }

    pub fn validate_base_dir(&self) -> Result<()> {
        if self.base_dir.as_os_str().is_empty() {
            bail!("base directory is empty");
        }
        if !self.base_dir.is_absolute() {
            bail!(
                "base directory must be an absolute path: {}",
                output::path(&self.base_dir)
            );
        }
        fs::create_dir_all(&self.base_dir).with_context(|| {
            format!(
                "cannot create base directory {}",
                output::path(&self.base_dir)
            )
        })
    }
}

const DEFAULT_CONFIG: &str = r#"{
  "base_dir": "~/Documents",
  "parallel": 4,
  "protocol": "ssh",
  "check_updates": true,
  "scan_network_mounts": false,
  "gitlab_statistics": true,
  "confirm_sync": "auto",
  "confirm_new_repos": 5,
  "confirm_download_gb": 1,
  "hosts": {
    "github":    ["github.com"],
    "gitlab":    ["gitlab.com"],
    "forgejo":   ["codeberg.org"],
    "bitbucket": ["bitbucket.org"]
  },
  "repos": {}
}
"#;

pub fn init_config() -> Result<()> {
    init_config_at(&config_path())
}

fn init_config_at(path: &Path) -> Result<()> {
    if path.exists() {
        output::warn(&format!("Config already exists: {}", output::path(path)));
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
    fs::write(path, DEFAULT_CONFIG)?;
    output::success(&format!("Config created: {}", output::path(path)));
    Ok(())
}

#[cfg(test)]
impl Settings {
    /// Defaults (no file, no env) rooted at `base`.
    pub fn for_tests(base: &Path) -> Self {
        let mut s = Self::load_from(Path::new(""), |_| None).unwrap();
        s.base_dir = base.to_path_buf();
        s
    }
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

    fn load(path: &Path) -> Settings {
        Settings::load_from(path, |_| None).unwrap()
    }

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            vars.iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn defaults_without_file() {
        let s = load(Path::new("/nonexistent/config.json"));
        assert_eq!(s.hosts(Provider::Forgejo), ["codeberg.org"]);
        assert_eq!(s.hosts(Provider::Radicle), ["radicle"]);
        assert_eq!(s.hosts(Provider::Sourcehut), ["git.sr.ht"]);
        assert!(s.hosts(Provider::Onedev).is_empty());
        assert!(s.hosts(Provider::Any).is_empty());
        assert_eq!(s.base_dir, home_dir().join("Documents"));
        assert_eq!(s.parallel, 4);
        assert!(s.check_updates);
        assert!(!s.use_https);
        assert!(!s.scan_network_mounts);
        assert!(s.gitlab_statistics);
    }

    #[test]
    fn hosts_string_and_list_forms() {
        let (_d, p) = write(r#"{"hosts":{"gitlab":"gl.example.org","forgejo":["a.org","b.org"]}}"#);
        let s = load(&p);
        assert_eq!(s.hosts(Provider::Gitlab), ["gl.example.org"]);
        assert_eq!(s.hosts(Provider::Forgejo), ["a.org", "b.org"]);
        assert_eq!(s.hosts(Provider::Github), ["github.com"]);
    }

    #[test]
    fn unknown_provider_and_empty_hosts_are_ignored() {
        let (_d, p) =
            write(r#"{"hosts":{"sourceforge":"sf.net","gitea":["", "g.org"],"github":[""]}}"#);
        let s = load(&p);
        assert_eq!(s.hosts(Provider::Forgejo), ["g.org"]);
        assert_eq!(s.hosts(Provider::Github), ["github.com"]);
    }

    #[test]
    fn plain_git_hosts_come_from_repos() {
        let (_d, p) = write(
            r#"{"repos":{
                "git.example.org":["git@git.example.org:infra/ansible.git"],
                "soft.example.org":{"discover":"soft-serve","port":23231},
                "empty.example.org":[],
                "":["git@x:y.git"]
            }}"#,
        );
        let s = load(&p);
        assert_eq!(
            s.hosts(Provider::Any),
            ["git.example.org", "soft.example.org"]
        );
        assert_eq!(
            s.any_host("git.example.org").unwrap().urls,
            ["git@git.example.org:infra/ansible.git"]
        );
        let soft = s.any_host("soft.example.org").unwrap();
        assert_eq!(soft.discover.as_deref(), Some("soft-serve"));
        assert_eq!(soft.port, Some(23231));
        assert!(s.any_host("empty.example.org").is_none());
    }

    #[test]
    fn repos_hosts_can_be_narrowed_by_hosts_any() {
        let (_d, p) = write(
            r#"{"hosts":{"any":["b.example.org"]},
                "repos":{"a.example.org":["git@a:x.git"],"b.example.org":["git@b:y.git"]}}"#,
        );
        let s = load(&p);
        assert_eq!(s.hosts(Provider::Any), ["b.example.org"]);
    }

    #[test]
    fn explicit_false_is_respected() {
        let (_d, p) =
            write(r#"{"check_updates":false,"scan_network_mounts":true,"protocol":"https"}"#);
        let s = load(&p);
        assert!(!s.check_updates);
        assert!(s.scan_network_mounts);
        let (_d, p) = write(r#"{"gitlab_statistics":false}"#);
        assert!(!load(&p).gitlab_statistics);
        assert!(s.use_https);
    }

    #[test]
    fn file_values_are_used() {
        let (_d, p) = write(r#"{"base_dir":"~/src","parallel":0}"#);
        let s = load(&p);
        assert_eq!(s.base_dir, home_dir().join("src"));
        assert_eq!(s.parallel, 1, "parallel is clamped to at least 1");
    }

    #[test]
    fn env_overrides_file() {
        let (_d, p) = write(r#"{"base_dir":"/from/file","parallel":2,"protocol":"https"}"#);
        let vars = [
            ("REPOS_MANAGER_BASE_DIR", "/from/env"),
            ("REPOS_MANAGER_PARALLEL", "9"),
            ("REPOS_MANAGER_PROTOCOL", "ssh"),
            ("REPOS_MANAGER_NO_UPDATE_CHECK", "1"),
        ];
        let s = Settings::load_from(&p, env(&vars)).unwrap();
        assert_eq!(s.base_dir, PathBuf::from("/from/env"));
        assert_eq!(s.parallel, 9);
        assert!(!s.use_https);
        assert!(!s.check_updates);
    }

    #[test]
    fn empty_or_invalid_env_falls_back_to_file() {
        let (_d, p) = write(r#"{"base_dir":"/from/file","parallel":3}"#);
        let vars = [
            ("REPOS_MANAGER_BASE_DIR", ""),
            ("REPOS_MANAGER_PARALLEL", "many"),
            ("REPOS_MANAGER_NO_UPDATE_CHECK", "0"),
        ];
        let s = Settings::load_from(&p, env(&vars)).unwrap();
        assert_eq!(s.base_dir, PathBuf::from("/from/file"));
        assert_eq!(s.parallel, 3);
        assert!(s.check_updates);
    }

    #[test]
    fn tilde_expansion() {
        assert_eq!(expand_tilde("~/Documents"), home_dir().join("Documents"));
        assert_eq!(expand_tilde(r"~\Documents"), home_dir().join("Documents"));
        assert_eq!(expand_tilde("/abs"), PathBuf::from("/abs"));
    }

    #[test]
    fn invalid_json_is_an_error() {
        let (_d, p) = write("{not json");
        assert!(Settings::load_from(&p, |_| None).is_err());
    }

    #[test]
    fn directory_config_is_treated_as_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(Settings::load_from(tmp.path(), |_| None).is_ok());
    }

    #[test]
    fn base_dir_validation() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Settings::for_tests(Path::new(""));
        assert!(s.validate_base_dir().is_err());
        s.base_dir = PathBuf::from("relative");
        let err = s.validate_base_dir().unwrap_err().to_string();
        assert!(err.contains("absolute"), "{err}");
        s.base_dir = tmp.path().join("a").join("b");
        s.validate_base_dir().unwrap();
        assert!(s.base_dir.is_dir());
        let file = tmp.path().join("file");
        fs::write(&file, "x").unwrap();
        s.base_dir = file.join("sub");
        assert!(s.validate_base_dir().is_err());
    }

    #[test]
    fn init_writes_default_config_once() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("config.json");
        init_config_at(&path).unwrap();
        let s = load(&path);
        assert_eq!(s.hosts(Provider::Bitbucket), ["bitbucket.org"]);
        assert!(!s.use_https);

        fs::write(&path, "{}").unwrap();
        init_config_at(&path).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{}",
            "existing file kept"
        );
    }
}
