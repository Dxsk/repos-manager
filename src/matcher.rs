//! Repo name filtering: `.repos.yml`, the legacy text files and the CLI flags.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Result, anyhow};
use glob::{MatchOptions, Pattern};
use serde::Deserialize;

use crate::output;

pub const CONFIG_FILE: &str = ".repos.yml";
const LEGACY_ONLY: &str = ".repos-filter";
const LEGACY_IGNORE: &str = ".repos-ignore";

const OPTS: MatchOptions = MatchOptions {
    case_sensitive: true,
    // `*` must cross `/` so `*/tmp-*` behaves like the original bash `[[ == ]]` match.
    require_literal_separator: false,
    require_literal_leading_dot: false,
};

/// Glob match. Patterns ending with `/*` also match nested paths, so
/// `group/*` covers `group/sub/repo` (GitLab subgroups).
pub fn match_pattern(pattern: &str, name: &str) -> bool {
    if let Ok(p) = Pattern::new(pattern)
        && p.matches_with(name, OPTS)
    {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return name.starts_with(&format!("{prefix}/"));
    }
    false
}

pub fn load_patterns(path: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// A key with nothing under it is an explicit YAML null, which `serde(default)`
/// rejects. `--write-ignore` emits exactly that (every entry commented out), so
/// null must read as empty or the tool could not load its own file.
fn nullable_list<'de, D>(de: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<String>>::deserialize(de)?.unwrap_or_default())
}

/// `.repos.yml`. Unknown keys are refused rather than ignored: a typo such as
/// `onlyy:` would silently widen the scope, which is what deletes
/// repositories under `--prune`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    only: Option<Vec<String>>,
    #[serde(default, deserialize_with = "nullable_list")]
    ignore: Vec<String>,
    #[serde(default)]
    hosts: HashMap<String, FileRules>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRules {
    #[serde(default)]
    only: Option<Vec<String>>,
    #[serde(default, deserialize_with = "nullable_list")]
    ignore: Vec<String>,
}

/// One allow-list, named after its key to trace an empty scope.
#[derive(Debug)]
struct Layer {
    source: String,
    patterns: Vec<String>,
}

impl Layer {
    fn new(source: &str, patterns: Vec<String>) -> Self {
        Self {
            source: source.to_string(),
            patterns,
        }
    }

    fn matches(&self, name: &str) -> bool {
        self.patterns.iter().any(|p| match_pattern(p, name))
    }
}

#[derive(Debug)]
struct HostRules {
    only: Option<Layer>,
    ignore: Vec<String>,
}

#[derive(Debug)]
pub struct RepoFilter {
    cli: Option<Layer>,
    /// `--ignore`, kept apart from the file's own deny-list so a rewrite of
    /// the file carries over its rules and not the flags of one run.
    cli_ignore: Vec<String>,
    global: Option<Layer>,
    ignore: Vec<String>,
    /// Keyed by lowercased host name: hostnames are case-insensitive, and so
    /// is the Windows filesystem.
    hosts: HashMap<String, HostRules>,
}

/// The rules that apply to one host. Allow-lists combine with AND and
/// deny-lists add up, so a host block can only ever narrow the global scope.
#[derive(Debug)]
pub struct HostFilter<'a> {
    only: Vec<&'a Layer>,
    ignore: Vec<&'a str>,
}

/// What one layer of the file holds, as `--write-ignore` has to write it back:
/// a rewrite that dropped an `only` list would widen the scope.
#[derive(Debug, Default, Clone, Copy)]
pub struct Rules<'a> {
    pub only: &'a [String],
    pub ignore: &'a [String],
}

pub enum Decision {
    Keep,
    Drop,
    Ignored,
}

/// An `only: []` empties the scope of everything it covers, and a generated
/// file must never do that by accident, so it warns instead of filtering.
fn layer(path: &Path, source: &str, patterns: Option<Vec<String>>) -> Option<Layer> {
    let patterns = patterns?;
    if patterns.is_empty() {
        output::warn(&format!(
            "{}: {source} is empty, it is not used as an allow-list",
            output::path(path)
        ));
        return None;
    }
    Some(Layer::new(source, patterns))
}

impl RepoFilter {
    pub fn load(base_dir: &Path, cli_only: &[String], cli_ignore: &[String]) -> Result<Self> {
        let mut filter = Self {
            cli: (!cli_only.is_empty()).then(|| Layer::new("--filter", cli_only.to_vec())),
            cli_ignore: cli_ignore.to_vec(),
            global: None,
            ignore: Vec::new(),
            hosts: HashMap::new(),
        };
        let path = base_dir.join(CONFIG_FILE);
        match fs::read_to_string(&path) {
            Ok(text) => filter.read_config(base_dir, &path, &text)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => filter.read_legacy(base_dir),
            // Falling back to "no filter" here would hand --prune the whole host.
            Err(e) => {
                return Err(anyhow!(e).context(format!("cannot read {}", output::path(&path))));
            }
        }
        Ok(filter)
    }

    /// `.repos.yml` replaces the text files outright: reading both would make
    /// the active rules depend on which file the user last remembered.
    fn read_config(&mut self, base_dir: &Path, path: &Path, text: &str) -> Result<()> {
        if [LEGACY_ONLY, LEGACY_IGNORE]
            .iter()
            .any(|n| base_dir.join(n).exists())
        {
            output::warn(&format!(
                "{} is in use, {LEGACY_ONLY} and {LEGACY_IGNORE} are ignored",
                output::path(path)
            ));
        }
        let config = serde_yaml_ng::from_str::<Option<FileConfig>>(text)
            .map_err(|e| anyhow!("invalid {}: {e}", output::path(path)))?
            .unwrap_or_default();
        self.global = layer(path, "only", config.only);
        self.ignore.extend(config.ignore);
        for (host, rules) in config.hosts {
            let source = format!("hosts.{host}.only");
            self.hosts.insert(
                host.to_lowercase(),
                HostRules {
                    only: layer(path, &source, rules.only),
                    ignore: rules.ignore,
                },
            );
        }
        Ok(())
    }

    /// The 1.0 text files, read exactly as they were: an existing but
    /// patternless `.repos-filter` still lets nothing through.
    fn read_legacy(&mut self, base_dir: &Path) {
        let file = base_dir.join(LEGACY_ONLY);
        if file.is_file() {
            self.global = Some(Layer::new(LEGACY_ONLY, load_patterns(&file)));
        }
        self.ignore
            .extend(load_patterns(&base_dir.join(LEGACY_IGNORE)));
    }

    pub fn for_host(&self, host: &str) -> HostFilter<'_> {
        let rules = self.hosts.get(&host.to_lowercase());
        let only = [
            self.cli.as_ref(),
            rules.and_then(|r| r.only.as_ref()),
            self.global.as_ref(),
        ];
        HostFilter {
            only: only.into_iter().flatten().collect(),
            ignore: self
                .cli_ignore
                .iter()
                .chain(&self.ignore)
                .chain(rules.into_iter().flat_map(|r| &r.ignore))
                .map(String::as_str)
                .collect(),
        }
    }

    pub fn global_rules(&self) -> Rules<'_> {
        Rules {
            only: self.global.as_ref().map_or(&[], |l| l.patterns.as_slice()),
            ignore: &self.ignore,
        }
    }

    pub fn host_rules(&self, host: &str) -> Rules<'_> {
        let Some(rules) = self.hosts.get(&host.to_lowercase()) else {
            return Rules::default();
        };
        Rules {
            only: rules.only.as_ref().map_or(&[], |l| l.patterns.as_slice()),
            ignore: &rules.ignore,
        }
    }
}

impl HostFilter<'_> {
    pub fn decide(&self, name: &str) -> Decision {
        if self.only.iter().any(|l| !l.matches(name)) {
            return Decision::Drop;
        }
        if self.ignore.iter().any(|p| match_pattern(p, name)) {
            return Decision::Ignored;
        }
        Decision::Keep
    }

    pub fn only_sources(&self) -> Vec<&str> {
        self.only.iter().map(|l| l.source.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(dir: &Path) -> RepoFilter {
        RepoFilter::load(dir, &[], &[]).unwrap()
    }

    fn write(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn exact_and_glob() {
        assert!(match_pattern("Dxsk/repo", "Dxsk/repo"));
        assert!(!match_pattern("Dxsk/repo", "Dxsk/other"));
        assert!(match_pattern("Dxsk/*", "Dxsk/repo"));
        assert!(match_pattern("*/tmp-*", "user/tmp-foo"));
        assert!(!match_pattern("Dxsk/*", "Other/repo"));
    }

    #[test]
    fn trailing_star_matches_subgroups() {
        assert!(match_pattern("group/*", "group/sub/repo"));
        assert!(!match_pattern("group/*", "groupie/repo"));
    }

    #[test]
    fn filter_and_ignore_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), LEGACY_ONLY, "# comment\nDxsk/*  # inline\n\n");
        write(dir.path(), LEGACY_IGNORE, "Dxsk/old\n");
        let f = load(dir.path());
        let h = f.for_host("github.com");
        assert!(matches!(h.decide("Dxsk/new"), Decision::Keep));
        assert!(matches!(h.decide("Dxsk/old"), Decision::Ignored));
        assert!(matches!(h.decide("Other/x"), Decision::Drop));
    }

    #[test]
    fn an_empty_legacy_filter_file_still_lets_nothing_through() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), LEGACY_ONLY, "# nothing but a comment\n");
        let f = load(dir.path());
        assert!(matches!(f.for_host("h").decide("a/b"), Decision::Drop));
    }

    #[test]
    fn no_files_keeps_everything() {
        let dir = tempfile::tempdir().unwrap();
        let f = RepoFilter::load(dir.path(), &["a/*".into()], &[]).unwrap();
        let h = f.for_host("h");
        assert!(matches!(h.decide("a/b"), Decision::Keep));
        assert!(matches!(h.decide("b/c"), Decision::Drop));
    }

    #[test]
    fn cli_patterns_are_one_or_list() {
        let dir = tempfile::tempdir().unwrap();
        let only = ["a/*".to_string(), "b/c".to_string()];
        let f = RepoFilter::load(dir.path(), &only, &["a/x".to_string()]).unwrap();
        let h = f.for_host("h");
        assert!(matches!(h.decide("a/b"), Decision::Keep));
        assert!(matches!(h.decide("b/c"), Decision::Keep));
        assert!(matches!(h.decide("a/x"), Decision::Ignored));
        assert!(matches!(h.decide("b/d"), Decision::Drop));
    }

    #[test]
    fn yaml_global_rules_apply_to_every_host() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            CONFIG_FILE,
            "only:\n  - Dxsk/*\nignore:\n  - Dxsk/old\n",
        );
        let f = load(dir.path());
        for host in ["github.com", "gitlab.corp"] {
            let h = f.for_host(host);
            assert!(matches!(h.decide("Dxsk/new"), Decision::Keep), "{host}");
            assert!(matches!(h.decide("Dxsk/old"), Decision::Ignored), "{host}");
            assert!(matches!(h.decide("other/x"), Decision::Drop), "{host}");
        }
    }

    #[test]
    fn a_host_block_narrows_that_host_only() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            CONFIG_FILE,
            "hosts:\n  gitlab.corp:\n    only: [team/*]\n    ignore: [team/archive-*]\n",
        );
        let f = load(dir.path());
        let corp = f.for_host("gitlab.corp");
        assert!(matches!(corp.decide("team/app"), Decision::Keep));
        assert!(matches!(corp.decide("team/archive-old"), Decision::Ignored));
        assert!(matches!(corp.decide("other/x"), Decision::Drop));
        // Another host is untouched by that block.
        let gh = f.for_host("github.com");
        assert!(matches!(gh.decide("other/x"), Decision::Keep));
        assert!(matches!(gh.decide("team/archive-old"), Decision::Keep));
    }

    #[test]
    fn a_host_block_can_never_widen_the_global_scope() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            CONFIG_FILE,
            "only: [Dxsk/*]\nignore: [Dxsk/old]\nhosts:\n  github.com:\n    only: [other/*]\n",
        );
        let f = load(dir.path());
        let h = f.for_host("github.com");
        // The host list cannot bring `other/*` in, nor bring `Dxsk/old` back.
        assert!(matches!(h.decide("other/x"), Decision::Drop));
        assert!(matches!(h.decide("Dxsk/new"), Decision::Drop));
        assert!(matches!(h.decide("Dxsk/old"), Decision::Drop));
        assert_eq!(h.only_sources(), ["hosts.github.com.only", "only"]);
    }

    #[test]
    fn host_keys_match_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            CONFIG_FILE,
            "hosts:\n  GitLab.Corp:\n    only: [team/*]\n",
        );
        let f = load(dir.path());
        assert!(matches!(
            f.for_host("gitlab.corp").decide("other/x"),
            Decision::Drop
        ));
        // The key keeps its spelling in messages.
        assert_eq!(
            f.for_host("GITLAB.CORP").only_sources(),
            ["hosts.GitLab.Corp.only"]
        );
    }

    #[test]
    fn an_empty_only_list_is_not_an_allow_list() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            CONFIG_FILE,
            "only: []\nhosts:\n  github.com:\n    only: []\n",
        );
        let f = load(dir.path());
        let h = f.for_host("github.com");
        assert!(matches!(h.decide("anything/at-all"), Decision::Keep));
        assert!(h.only_sources().is_empty());
    }

    #[test]
    fn every_allow_list_layer_is_named() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            CONFIG_FILE,
            "only: [a/*]\nhosts:\n  h:\n    only: [b/*]\n",
        );
        let f = RepoFilter::load(dir.path(), &["c/*".to_string()], &[]).unwrap();
        assert_eq!(
            f.for_host("h").only_sources(),
            ["--filter", "hosts.h.only", "only"]
        );
    }

    #[test]
    fn the_yaml_file_replaces_the_legacy_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), LEGACY_ONLY, "nothing/matches-this\n");
        write(dir.path(), LEGACY_IGNORE, "a/b\n");
        write(dir.path(), CONFIG_FILE, "only: [a/*]\n");
        let f = load(dir.path());
        assert!(matches!(f.for_host("h").decide("a/b"), Decision::Keep));
    }

    #[test]
    fn an_unknown_key_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), CONFIG_FILE, "onlyy:\n  - a/*\n");
        let e = RepoFilter::load(dir.path(), &[], &[]).unwrap_err();
        let msg = format!("{e:#}");
        assert!(msg.contains(CONFIG_FILE), "{msg}");
        assert!(msg.contains("onlyy"), "{msg}");
    }

    #[test]
    fn a_malformed_file_names_itself_and_aborts() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), CONFIG_FILE, "only: [a/*\n");
        let e = RepoFilter::load(dir.path(), &[], &[]).unwrap_err();
        assert!(format!("{e:#}").contains(CONFIG_FILE), "{e:#}");
    }

    /// `--write-ignore` emits an `ignore:` whose every entry is commented out,
    /// which YAML reads as an explicit null.
    #[test]
    fn a_key_with_nothing_under_it_is_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            CONFIG_FILE,
            "ignore:\nhosts:\n  github.com:\n    ignore:\n",
        );
        let f = load(dir.path());
        assert!(matches!(
            f.for_host("github.com").decide("a/b"),
            Decision::Keep
        ));
        assert!(f.global_rules().ignore.is_empty());
        assert!(f.host_rules("github.com").ignore.is_empty());
    }

    /// The first guarantee of a generated file: delivered as it was written,
    /// it changes nothing about what gets synced.
    #[test]
    fn a_generated_file_keeps_every_repository_it_lists() {
        use crate::plan::{HostRow, Plan, ScopeRepo};

        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            CONFIG_FILE,
            "only: ['Dxsk/*', '~dxsk/*']\nignore: [Dxsk/old]\nhosts:\n  git.sr.ht:\n    ignore: ['~dxsk/secret']\n",
        );
        let host = |host: &str, names: &[&str]| HostRow {
            host: host.to_string(),
            repos: names
                .iter()
                .enumerate()
                .map(|(i, name)| ScopeRepo {
                    name: (*name).to_string(),
                    size_kb: (i > 0).then_some(1024 * i as u64),
                    cloned: i == 1,
                })
                .collect(),
            ..Default::default()
        };
        let plan = Plan {
            hosts: vec![
                host("github.com", &["Dxsk/a", "Dxsk/b", "Dxsk/o'hara"]),
                host("git.sr.ht", &["~dxsk/dotfiles"]),
            ],
            prune: false,
        };

        let generated = crate::plan::ignore_file(&plan, &load(dir.path()), "2026-10-11");
        write(dir.path(), CONFIG_FILE, &generated);
        let f = load(dir.path());
        for row in &plan.hosts {
            let h = f.for_host(&row.host);
            for repo in &row.repos {
                assert!(
                    matches!(h.decide(&repo.name), Decision::Keep),
                    "{} dropped by\n{generated}",
                    repo.name
                );
            }
        }
        // And the rules it carried over are still in force.
        assert!(matches!(
            f.for_host("github.com").decide("other/x"),
            Decision::Drop
        ));
        assert!(matches!(
            f.for_host("git.sr.ht").decide("~dxsk/secret"),
            Decision::Ignored
        ));
    }

    #[test]
    fn an_empty_file_filters_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), CONFIG_FILE, "# only comments\n");
        let f = load(dir.path());
        assert!(matches!(f.for_host("h").decide("a/b"), Decision::Keep));
    }
}
