//! Repo name filtering: `--filter`, `.repos-filter` (allow-list) and
//! `.repos-ignore` (deny-list), both read from the base directory.

use std::fs;
use std::path::Path;

use glob::{MatchOptions, Pattern};

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

/// Read patterns from a file, stripping `#` comments and blank lines.
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

pub struct RepoFilter {
    cli_filter: Option<String>,
    allow: Option<Vec<String>>,
    ignore: Vec<String>,
}

pub enum Decision {
    Keep,
    /// Silently dropped (`--filter` or `.repos-filter` miss).
    Drop,
    /// Explicitly ignored via `.repos-ignore`, reported as skipped.
    Ignored,
}

impl RepoFilter {
    pub fn load(base_dir: &Path, cli_filter: Option<String>) -> Self {
        let filter_file = base_dir.join(".repos-filter");
        let allow = filter_file.is_file().then(|| load_patterns(&filter_file));
        Self {
            cli_filter,
            allow,
            ignore: load_patterns(&base_dir.join(".repos-ignore")),
        }
    }

    pub fn decide(&self, name: &str) -> Decision {
        if let Some(f) = &self.cli_filter
            && !match_pattern(f, name)
        {
            return Decision::Drop;
        }
        if let Some(allow) = &self.allow
            && !allow.iter().any(|p| match_pattern(p, name))
        {
            return Decision::Drop;
        }
        if self.ignore.iter().any(|p| match_pattern(p, name)) {
            return Decision::Ignored;
        }
        Decision::Keep
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        fs::write(
            dir.path().join(".repos-filter"),
            "# comment\nDxsk/*  # inline\n\n",
        )
        .unwrap();
        fs::write(dir.path().join(".repos-ignore"), "Dxsk/old\n").unwrap();
        let f = RepoFilter::load(dir.path(), None);
        assert!(matches!(f.decide("Dxsk/new"), Decision::Keep));
        assert!(matches!(f.decide("Dxsk/old"), Decision::Ignored));
        assert!(matches!(f.decide("Other/x"), Decision::Drop));
    }

    #[test]
    fn no_files_keeps_everything() {
        let dir = tempfile::tempdir().unwrap();
        let f = RepoFilter::load(dir.path(), Some("a/*".into()));
        assert!(matches!(f.decide("a/b"), Decision::Keep));
        assert!(matches!(f.decide("b/c"), Decision::Drop));
    }
}
