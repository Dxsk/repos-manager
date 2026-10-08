use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::config::Settings;
use crate::git;
use crate::output::{self, blue, gray, green, red, yellow};

/// Vendored or build directories that can hold thousands of entries and never contain synced repos.
const HEAVY_DIRS: &[&str] = &[
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    "target",
    "vendor",
    "dist",
    "build",
    ".next",
    ".cache",
];

/// Network or userspace filesystems where a recursive walk turns into
/// thousands of remote round-trips (cloud drives, NFS, SMB, sshfs...).
fn is_network_fstype(fstype: &str) -> bool {
    let base = fstype.split('.').next().unwrap_or(fstype);
    let trimmed = base.trim_end_matches(|c: char| c.is_ascii_digit());
    matches!(
        trimmed,
        "fuse" | "nfs" | "cifs" | "smb" | "smbfs" | "afs" | "ceph" | "davfs"
    )
}

/// Parse `/proc/self/mountinfo` content and return network mount points under `base`.
/// Fields: id parent major:minor root mount_point options [optional...] - fstype source super_options
pub fn network_mount_points(mountinfo: &str, base: &Path) -> Vec<PathBuf> {
    mountinfo
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let mount_point = fields.get(4)?;
            let sep = fields.iter().skip(6).position(|f| *f == "-")? + 6;
            let fstype = fields.get(sep + 1)?;
            let mp = PathBuf::from(mount_point.replace("\\040", " "));
            (is_network_fstype(fstype) && mp.starts_with(base) && mp != base).then_some(mp)
        })
        .collect()
}

fn excluded_mounts(settings: &Settings) -> Vec<PathBuf> {
    if settings.scan_network_mounts {
        output::warn(
            "status: scanning network mounts (can be very slow or hang on unreliable links)",
        );
        return Vec::new();
    }
    let mounts = std::fs::read_to_string("/proc/self/mountinfo")
        .map(|s| network_mount_points(&s, &settings.base_dir))
        .unwrap_or_default();
    for m in &mounts {
        output::debug(&format!("status: pruning network mount {}", m.display()));
    }
    mounts
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RepoState {
    pub dirty: bool,
    pub ahead: u32,
    pub behind: u32,
}

pub fn repo_state(dir: &Path) -> RepoState {
    let mut st = RepoState {
        dirty: git::is_dirty(dir),
        ..Default::default()
    };
    if git::run(dir, &["rev-parse", "--abbrev-ref", "@{upstream}"]).is_ok()
        && let Ok(ab) = git::run(
            dir,
            &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
        )
    {
        let mut it = ab.split_whitespace().map(|n| n.parse().unwrap_or(0));
        st.ahead = it.next().unwrap_or(0);
        st.behind = it.next().unwrap_or(0);
    }
    st
}

pub fn status_all(settings: &Settings) {
    let base = &settings.base_dir;
    output::info(&format!("Scanning repos in {}...", base.display()));
    output::blank();

    let show_progress = std::io::stderr().is_terminal() && !output::is_quiet();
    let skip_mounts = excluded_mounts(settings);
    let (mut total, mut clean, mut dirty, mut ahead, mut behind, mut diverged) = (0, 0, 0, 0, 0, 0);

    let mut it = WalkDir::new(base).follow_links(false).into_iter();
    while let Some(entry) = it.next() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if HEAVY_DIRS.contains(&name.as_ref()) || skip_mounts.iter().any(|m| m == entry.path()) {
            it.skip_current_dir();
            continue;
        }
        if name != ".git" {
            continue;
        }
        it.skip_current_dir();
        let Some(repo) = entry.path().parent() else {
            continue;
        };
        let rel = repo
            .strip_prefix(base)
            .unwrap_or(repo)
            .display()
            .to_string();
        total += 1;

        if show_progress {
            let shown = if rel.chars().count() > 70 {
                let tail: String = rel
                    .chars()
                    .rev()
                    .take(69)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                format!("…{tail}")
            } else {
                rel.clone()
            };
            eprint!("\r  {} {shown}\x1b[K", gray(&format!("[{total}]")));
            let _ = std::io::stderr().flush();
        }

        let st = repo_state(repo);
        let mut flags = Vec::new();
        if st.dirty {
            flags.push(yellow("dirty"));
            dirty += 1;
        }
        match (st.ahead, st.behind) {
            (0, 0) => {}
            (a, 0) => {
                flags.push(format!("{} (+{a})", green("ahead")));
                ahead += 1;
            }
            (0, b) => {
                flags.push(format!("{} (-{b})", blue("behind")));
                behind += 1;
            }
            (a, b) => {
                flags.push(format!("{} (+{a}/-{b})", red("diverged")));
                diverged += 1;
            }
        }

        if flags.is_empty() {
            clean += 1;
        } else {
            if show_progress {
                eprint!("\r\x1b[K");
            }
            println!("  {rel} {}", flags.join(" "));
        }
    }
    if show_progress {
        eprint!("\r\x1b[K");
    }

    output::blank();
    output::info(&format!(
        "Total: {total} repos - {}, {}, {}, {}, {}",
        green(&format!("{clean} clean")),
        yellow(&format!("{dirty} dirty")),
        green(&format!("{ahead} ahead")),
        blue(&format!("{behind} behind")),
        red(&format!("{diverged} diverged")),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::tests::{bare_remote, git as git_cmd};

    #[test]
    fn mountinfo_parsing() {
        let info = "\
36 35 98:0 / /home/u/Documents/kDrive rw,nosuid shared:1 - fuse.kDrive kDrive rw
37 35 98:0 / /home/u/Documents/nfs rw - nfs4 server:/x rw
38 35 98:0 / /home/u/Documents/local rw - ext4 /dev/sda1 rw
39 35 98:0 / /mnt/other rw - cifs //srv/share rw
";
        let got = network_mount_points(info, Path::new("/home/u/Documents"));
        assert_eq!(
            got,
            vec![
                PathBuf::from("/home/u/Documents/kDrive"),
                PathBuf::from("/home/u/Documents/nfs")
            ]
        );
    }

    #[test]
    fn fstype_classification() {
        for t in [
            "fuse",
            "fuse.sshfs",
            "nfs",
            "nfs4",
            "cifs",
            "smb3",
            "davfs",
            "ceph",
        ] {
            assert!(is_network_fstype(t), "{t}");
        }
        for t in ["ext4", "btrfs", "tmpfs", "overlay"] {
            assert!(!is_network_fstype(t), "{t}");
        }
    }

    #[test]
    fn detects_dirty_and_ahead() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let clone = tmp.path().join("clone");
        git_cmd(
            tmp.path(),
            &[
                "clone",
                "--quiet",
                bare.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        );
        assert_eq!(repo_state(&clone), RepoState::default());

        git_cmd(
            &clone,
            &["commit", "--quiet", "--allow-empty", "-m", "local"],
        );
        std::fs::write(clone.join("f.txt"), "x").unwrap();
        assert_eq!(
            repo_state(&clone),
            RepoState {
                dirty: true,
                ahead: 1,
                behind: 0
            }
        );
    }
}
