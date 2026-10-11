//! Where repos live on disk and what the filesystem makes of their names.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

use crate::git;

/// Hosts may carry a port (`git.example.org:3000`), which is not a valid
/// directory name on Windows.
pub fn host_dir(base: &Path, host: &str) -> PathBuf {
    if cfg!(windows) {
        base.join(host.replace(':', "_"))
    } else {
        base.join(host)
    }
}

/// Repo names come from a remote API: on Windows a `\\` separator or a `C:`
/// drive segment would otherwise place a clone outside the base directory.
pub(super) fn repo_path(host_dir: &Path, full_name: &str) -> PathBuf {
    full_name
        .split(['/', '\\'])
        .filter(|c| {
            if c.contains(':') {
                return false;
            }
            let mut parts = Path::new(c).components();
            matches!(
                (parts.next(), parts.next()),
                (Some(Component::Normal(_)), None)
            )
        })
        .fold(host_dir.to_path_buf(), |p, c| p.join(c))
}

fn is_partial_clone(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with('.') && name.ends_with(".partial")
}

pub fn find_repos(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut it = WalkDir::new(root).follow_links(false).into_iter();
    while let Some(Ok(entry)) = it.next() {
        if is_partial_clone(entry.file_name()) {
            it.skip_current_dir();
            continue;
        }
        if entry.file_type().is_dir() && entry.file_name() == ".git" {
            if let Some(parent) = entry.path().parent() {
                out.push(parent.to_path_buf());
            }
            it.skip_current_dir();
        }
    }
    out
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Fs {
    /// Windows refuses some names outright (`aux`, a trailing dot).
    windows: bool,
    /// NTFS and APFS fold case, ext4 does not.
    case_insensitive: bool,
    /// APFS and HFS+ compare Unicode-normalized: NFC and NFD `é` are one directory
    /// there, two on NTFS and ext4.
    normalizing: bool,
}

impl Fs {
    pub(super) const fn current() -> Self {
        Self {
            windows: cfg!(windows),
            case_insensitive: cfg!(any(windows, target_os = "macos")),
            normalizing: cfg!(any(target_os = "macos", target_os = "ios")),
        }
    }

    /// Lowercasing can decompose (`İ` becomes `i` plus a combining dot), so
    /// normalizing again afterwards keeps the key canonical.
    fn key(&self, name: &str) -> String {
        let normalize = |s: String| {
            if self.normalizing {
                s.nfc().collect()
            } else {
                s
            }
        };
        let folded = normalize(name.to_string());
        if self.case_insensitive {
            normalize(folded.to_lowercase())
        } else {
            folded
        }
    }
}

/// Distinct repos must land in distinct, non-nested directories as the
/// filesystem sees them, or one clone silently stands in for another. Returns
/// the losers by index; the repo already on disk wins.
pub(super) fn path_conflicts(
    repos: &[(String, &Path, String)],
    target: Fs,
    origin: impl Fn(&Path) -> Option<String>,
) -> HashMap<usize, String> {
    let mut losers = HashMap::new();
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, (name, _, _)) in repos.iter().enumerate() {
        if target.windows && git::invalid_on_windows(name) {
            losers.insert(i, "name not allowed on Windows".to_string());
            continue;
        }
        // Windows would also drop trailing dots, but invalid_on_windows rejects those.
        groups.entry(target.key(name)).or_default().push(i);
    }
    let winner = |members: &[usize]| {
        let (_, path, _) = &repos[members[0]];
        let on_disk = origin(path);
        members
            .iter()
            .copied()
            .find(|&m| on_disk.as_deref() == Some(repos[m].2.as_str()))
            .unwrap_or(members[0])
    };
    for (key, members) in &groups {
        // The outermost ancestor has no ancestor itself, so its winner is kept.
        let ancestor = key
            .match_indices('/')
            .find_map(|(cut, _)| groups.get(&key[..cut]));
        if let Some(parent) = ancestor {
            let reason = format!("nested inside {}", repos[winner(parent)].0);
            losers.extend(members.iter().map(|&m| (m, reason.clone())));
        } else if members.len() > 1 {
            let w = winner(members);
            let reason = format!("same directory as {}", repos[w].0);
            losers.extend(
                members
                    .iter()
                    .filter(|&&m| m != w)
                    .map(|&m| (m, reason.clone())),
            );
        }
    }
    losers
}

pub(super) fn origin_url(repo: &Path) -> Option<String> {
    git::run(repo, &["remote", "get-url", "origin"]).ok()
}

pub(super) fn repo_name(host_dir: &Path, repo: &Path) -> String {
    let rel = repo.strip_prefix(host_dir).unwrap_or(repo);
    let parts: Vec<_> = rel.iter().map(|c| c.to_string_lossy()).collect();
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::jobs::sync_repo;
    use crate::sync::tests::bare_remote;
    use std::fs;

    #[test]
    fn repo_path_rejects_traversal() {
        let host = Path::new("/b/h");
        let expected = host.join("etc").join("x");
        for name in [
            "../../etc/x",
            "..\\..\\etc\\x",
            "./etc/./x",
            "etc/../x/..",
            "/etc//x",
            "C:\\etc\\x",
            "C:/etc/x",
        ] {
            assert_eq!(repo_path(host, name), expected, "{name}");
        }
        let unc = repo_path(host, "\\\\server\\share");
        assert_eq!(unc, host.join("server").join("share"));
    }

    #[test]
    fn partial_clones_are_not_repos() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("u").join(".r.partial").join(".git")).unwrap();
        assert!(find_repos(tmp.path()).is_empty());
    }

    const LINUX: Fs = Fs {
        windows: false,
        case_insensitive: false,
        normalizing: false,
    };
    const MACOS: Fs = Fs {
        windows: false,
        case_insensitive: true,
        normalizing: true,
    };
    const WINDOWS: Fs = Fs {
        windows: true,
        case_insensitive: true,
        normalizing: false,
    };

    fn conflicts_of(names: &[&str], target: Fs) -> Vec<(usize, String)> {
        let repos: Vec<_> = names
            .iter()
            .map(|n| (n.to_string(), Path::new("/nonexistent"), String::new()))
            .collect();
        let mut out: Vec<_> = path_conflicts(&repos, target, |_| None)
            .into_iter()
            .collect();
        out.sort();
        out
    }

    #[test]
    fn path_conflicts_follow_the_filesystem() {
        let names = [
            "u/Repo", "u/repo", "u/trail.", "u/trail", "g/p", "g/p/sub", "u/aux",
        ];
        assert_eq!(
            conflicts_of(&names, LINUX),
            [(5, "nested inside g/p".to_string())]
        );
        let mac = conflicts_of(&names, MACOS);
        assert_eq!(mac[0], (1, "same directory as u/Repo".into()));
        assert_eq!(mac.len(), 2);
        let win = conflicts_of(&names, WINDOWS);
        assert_eq!(
            win,
            [
                (1, "same directory as u/Repo".into()),
                (2, "name not allowed on Windows".into()),
                (5, "nested inside g/p".into()),
                (6, "name not allowed on Windows".into()),
            ]
        );
    }

    /// `Fs::current()` is a claim about the running filesystem, so probe it.
    /// Only one direction has to hold: missing a collision lets one clone
    /// stand in for another, while an extra one merely skips a repository.
    #[test]
    fn the_collision_key_folds_at_least_as_much_as_the_filesystem() {
        let tmp = tempfile::tempdir().unwrap();
        let cur = Fs::current();
        let conflated = |a: &str, b: &str| {
            let dir = tmp.path().join(a);
            fs::create_dir_all(&dir).unwrap();
            let shared = tmp.path().join(b).is_dir();
            fs::remove_dir_all(&dir).unwrap();
            !shared || cur.key(a) == cur.key(b)
        };
        assert!(conflated("Case", "case"), "case folding missed");
        assert!(conflated("caf\u{e9}", "cafe\u{301}"), "NFC/NFD missed");
    }

    #[test]
    fn unicode_spellings_collide_only_where_the_filesystem_normalizes() {
        // The same word, precomposed (NFC) then decomposed (NFD).
        let names = ["u/caf\u{e9}", "u/cafe\u{301}"];
        assert!(conflicts_of(&names, LINUX).is_empty());
        assert!(conflicts_of(&names, WINDOWS).is_empty());
        assert_eq!(
            conflicts_of(&names, MACOS),
            [(1, "same directory as u/caf\u{e9}".to_string())]
        );
        let nested = ["u/caf\u{e9}", "u/cafe\u{301}/sub"];
        assert_eq!(
            conflicts_of(&nested, MACOS),
            [(1, "nested inside u/caf\u{e9}".to_string())]
        );
    }

    /// Shrunk from a proptest failure: every name in a nested chain must lose
    /// to the outermost one, never to another loser.
    #[test]
    fn a_nested_chain_loses_to_the_outermost_name() {
        assert_eq!(
            conflicts_of(&["a", "a/a", "a/a/a"], LINUX),
            [
                (1, "nested inside a".to_string()),
                (2, "nested inside a".to_string()),
            ]
        );
    }

    #[test]
    fn existing_clone_keeps_its_directory_on_collision() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let dest = tmp.path().join("u").join("trail");
        sync_repo(&dest, bare.to_str().unwrap());
        let repos = vec![
            (
                "u/Trail".to_string(),
                dest.as_path(),
                "https://other".to_string(),
            ),
            (
                "u/trail".to_string(),
                dest.as_path(),
                bare.to_str().unwrap().to_string(),
            ),
        ];
        let losers = path_conflicts(&repos, WINDOWS, origin_url);
        assert_eq!(losers.keys().collect::<Vec<_>>(), [&0]);
    }

    #[test]
    fn host_dir_escapes_port_on_windows() {
        let d = host_dir(Path::new("base"), "h.org:3000");
        let expected = if cfg!(windows) {
            "h.org_3000"
        } else {
            "h.org:3000"
        };
        assert_eq!(d, Path::new("base").join(expected));
    }

    #[test]
    fn find_repos_does_not_descend_into_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("a");
        fs::create_dir_all(outer.join(".git").join("modules").join("x").join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join("b").join("c").join(".git")).unwrap();
        let mut found = find_repos(tmp.path());
        found.sort();
        assert_eq!(found, [outer, tmp.path().join("b").join("c")]);
    }
}

#[cfg(test)]
mod properties {
    use super::*;
    use proptest::prelude::*;

    fn names() -> impl Strategy<Value = Vec<String>> {
        // A tiny alphabet so collisions, nesting and reserved names show up
        // often. The last two spell one word NFC then NFD: distinct code
        // points that a normalizing filesystem resolves to one directory.
        let segment = prop::sample::select(vec![
            "a",
            "A",
            "b",
            "aux",
            "x.",
            "p",
            "caf\u{e9}",
            "cafe\u{301}",
        ]);
        let name = prop::collection::vec(segment, 1..4).prop_map(|s| s.join("/"));
        prop::collection::btree_set(name, 0..12).prop_map(|s| s.into_iter().collect())
    }

    fn filesystems() -> impl Strategy<Value = Fs> {
        (any::<bool>(), any::<bool>(), any::<bool>()).prop_map(|(w, ci, n)| Fs {
            windows: w,
            case_insensitive: ci,
            normalizing: n,
        })
    }

    fn is_proper_prefix(parent: &str, child: &str) -> bool {
        child.len() > parent.len()
            && child.starts_with(parent)
            && child.as_bytes()[parent.len()] == b'/'
    }

    proptest! {
        #[test]
        fn repo_path_never_leaves_the_host_dir(name in r"[a-zA-Z0-9._:/\\ -]{0,40}") {
            let host = Path::new("/b/h");
            let p = repo_path(host, &name);
            let rel = p.strip_prefix(host).unwrap();
            for c in rel.components() {
                prop_assert!(matches!(c, Component::Normal(_)), "{name:?} gave {p:?}");
            }
            prop_assert!(!rel.to_string_lossy().contains(':'));
        }

        #[test]
        fn path_conflicts_leave_an_injective_prefix_free_set(
            names in names(),
            target in filesystems(),
        ) {
            let repos: Vec<_> = names
                .iter()
                .map(|n| (n.clone(), Path::new("/nonexistent"), String::new()))
                .collect();
            let losers = path_conflicts(&repos, target, |_| None);
            // The key comes from the code under test: the property is about
            // the conflict algorithm, not about what the filesystem folds.
            let key = |i: usize| target.key(&names[i]);
            let winners: Vec<usize> = (0..names.len()).filter(|i| !losers.contains_key(i)).collect();

            for (x, &i) in winners.iter().enumerate() {
                prop_assert!(!(target.windows && git::invalid_on_windows(&names[i])));
                for &j in &winners[x + 1..] {
                    prop_assert_ne!(key(i), key(j), "{} and {} share a directory", names[i], names[j]);
                    prop_assert!(!is_proper_prefix(&key(i), &key(j)) && !is_proper_prefix(&key(j), &key(i)),
                        "{} and {} nest", names[i], names[j]);
                }
            }
            // Every loser is justified by a winner, so nothing is dropped for no reason.
            for (&i, reason) in &losers {
                if reason == "name not allowed on Windows" {
                    prop_assert!(target.windows && git::invalid_on_windows(&names[i]));
                    continue;
                }
                let (kind, other) = reason.split_once(' ').map(|(_, r)| r.rsplit_once(' ').unwrap()).unwrap();
                let w = winners.iter().copied().find(|&w| names[w] == other);
                prop_assert!(w.is_some(), "{} lost to {other}, which is not kept", names[i]);
                let w = w.unwrap();
                match kind {
                    "directory as" => prop_assert_eq!(key(w), key(i)),
                    "inside" => prop_assert!(is_proper_prefix(&key(w), &key(i))),
                    _ => prop_assert!(false, "unknown reason {reason}"),
                }
            }
        }
    }
}
