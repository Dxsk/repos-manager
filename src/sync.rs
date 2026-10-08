use std::collections::HashSet;
use std::fs::{self, File, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;

use anyhow::{Result, bail};
use walkdir::WalkDir;

use crate::config::Settings;
use crate::git;
use crate::matcher::{Decision, RepoFilter};
use crate::output;
use crate::providers::{ListError, Provider};

#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    pub filter: Option<String>,
    pub prune: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Cloned,
    Updated,
    Skipped,
    Errored,
}

/// Exclusive per-host lock: distinct hosts write to disjoint subtrees and may
/// sync concurrently. The OS drops the lock if the process dies, so a stale
/// lockfile never blocks the next run.
pub struct HostLock {
    file: Option<File>,
    path: PathBuf,
}

impl HostLock {
    pub fn acquire(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(".repos-manager.lock");
        let mut file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                bail!("another sync is already running on {}", dir.display())
            }
            Err(TryLockError::Error(e)) => return Err(e.into()),
        }
        // Write through the locked handle: Windows rejects writes from any other handle.
        file.set_len(0)?;
        write!(file, "{}", std::process::id())?;
        Ok(Self {
            file: Some(file),
            path,
        })
    }
}

impl Drop for HostLock {
    fn drop(&mut self) {
        // Close first: Windows cannot delete a file that is still open.
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

/// Hosts may carry a port (`git.example.org:3000`), which is not a valid
/// directory name on Windows.
pub fn host_dir(base: &Path, host: &str) -> PathBuf {
    if cfg!(windows) {
        base.join(host.replace(':', "_"))
    } else {
        base.join(host)
    }
}

fn repo_path(host_dir: &Path, full_name: &str) -> PathBuf {
    full_name
        .split('/')
        .filter(|c| !c.is_empty() && *c != "." && *c != "..")
        .fold(host_dir.to_path_buf(), |p, c| p.join(c))
}

pub fn sync_repo(local_path: &Path, clone_url: &str, full_name: &str) -> Outcome {
    if local_path.join(".git").is_dir() {
        if git::is_dirty(local_path) {
            output::warn(&format!("{full_name} (dirty, skipped)"));
            return Outcome::Skipped;
        }
        // Pull origin HEAD explicitly so clones without upstream tracking
        // (manual clones, renamed default branch) still update.
        let res = git::run(local_path, &["fetch", "--all", "--quiet"]).and_then(|_| {
            git::run(
                local_path,
                &["pull", "--ff-only", "--quiet", "origin", "HEAD"],
            )
        });
        match res {
            Ok(_) => {
                output::success(&format!("{full_name} (updated)"));
                Outcome::Updated
            }
            Err(e) => {
                output::error(&format!("{full_name} (update failed): {e}"));
                Outcome::Errored
            }
        }
    } else {
        if let Some(parent) = local_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let parent = local_path.parent().unwrap_or(Path::new("."));
        let target = local_path.to_string_lossy();
        match git::run(parent, &["clone", "--quiet", clone_url, &target]) {
            Ok(_) => {
                output::success(&format!("{full_name} (cloned)"));
                Outcome::Cloned
            }
            Err(e) => {
                output::error(&format!("{full_name} (clone failed): {e}"));
                Outcome::Errored
            }
        }
    }
}

fn run_parallel(jobs: Vec<(PathBuf, String, String)>, parallel: usize) -> Vec<Outcome> {
    let queue = Mutex::new(jobs.into_iter());
    let results = Mutex::new(Vec::new());
    thread::scope(|s| {
        for _ in 0..parallel {
            s.spawn(|| {
                loop {
                    let next = queue.lock().unwrap().next();
                    let Some((path, url, name)) = next else { break };
                    let outcome = sync_repo(&path, &url, &name);
                    results.lock().unwrap().push(outcome);
                }
            });
        }
    });
    results.into_inner().unwrap()
}

pub fn sync_host(
    provider: Provider,
    host: &str,
    settings: &Settings,
    opts: &SyncOptions,
) -> Result<()> {
    let dir = host_dir(&settings.base_dir, host);
    let _lock = HostLock::acquire(&dir)?;

    output::info(&format!("Fetching repository list from {host}..."));
    let repos = match provider.list_repos(host) {
        Ok(r) => r,
        Err(ListError::Skip(reason)) => {
            output::warn(&format!("Skipping {host}: {reason}"));
            return Ok(());
        }
        Err(ListError::Fail(e)) => return Err(e),
    };
    output::info(&format!("Found {} repositories", repos.len()));
    output::blank();

    let filter = RepoFilter::load(&settings.base_dir, opts.filter.clone());
    let mut skipped = 0;
    let mut synced: HashSet<PathBuf> = HashSet::new();
    let mut jobs = Vec::new();

    for repo in repos {
        let full_name = &repo.full_name;
        match filter.decide(full_name) {
            Decision::Drop => continue,
            Decision::Ignored => {
                output::skip(&format!("{full_name} (ignored)"));
                skipped += 1;
                continue;
            }
            Decision::Keep => {}
        }
        let path = repo_path(&dir, full_name);
        synced.insert(path.clone());
        if opts.dry_run {
            let verb = if path.join(".git").is_dir() {
                "update"
            } else {
                "clone"
            };
            output::info(&format!("  [dry-run] would {verb} {full_name}"));
            continue;
        }
        jobs.push((
            path,
            repo.clone_url(settings.use_https).to_string(),
            full_name.clone(),
        ));
    }

    let outcomes = run_parallel(jobs, settings.parallel);
    let count = |o: Outcome| outcomes.iter().filter(|x| **x == o).count();

    if opts.prune {
        if opts.filter.is_some() {
            output::warn("Pruning skipped: not supported with --filter");
        } else {
            prune(&settings.base_dir, &dir, &synced, opts.dry_run)?;
        }
    }

    output::blank();
    output::info(&format!(
        "Done: {} cloned, {} updated, {} skipped, {} errors",
        count(Outcome::Cloned),
        count(Outcome::Updated),
        count(Outcome::Skipped) + skipped,
        count(Outcome::Errored),
    ));
    Ok(())
}

/// Find git working copies under `root` without descending into `.git` itself.
pub fn find_repos(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut it = WalkDir::new(root).follow_links(false).into_iter();
    while let Some(Ok(entry)) = it.next() {
        if entry.file_type().is_dir() && entry.file_name() == ".git" {
            if let Some(parent) = entry.path().parent() {
                out.push(parent.to_path_buf());
            }
            it.skip_current_dir();
        }
    }
    out
}

pub fn prune(base: &Path, host_dir: &Path, synced: &HashSet<PathBuf>, dry_run: bool) -> Result<()> {
    if !host_dir.is_dir() {
        return Ok(());
    }
    let (real_host, real_base) = (host_dir.canonicalize()?, base.canonicalize()?);
    if !real_host.starts_with(&real_base) || real_host == real_base {
        bail!(
            "prune aborted: {} is outside the base directory",
            host_dir.display()
        );
    }

    let mut pruned = 0;
    for repo in find_repos(host_dir) {
        if synced.contains(&repo) {
            continue;
        }
        let rel = repo
            .strip_prefix(base)
            .unwrap_or(&repo)
            .display()
            .to_string();
        if dry_run {
            output::warn(&format!("[dry-run] would prune {rel}"));
        } else {
            output::error(&format!("{rel} (pruned)"));
            fs::remove_dir_all(&repo)?;
        }
        pruned += 1;
    }
    if pruned > 0 {
        output::warn(&format!("Pruned {pruned} repositories"));
    }
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::process::Command;

    pub fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }

    /// Bare remote with one commit on `main`.
    pub fn bare_remote(tmp: &Path) -> PathBuf {
        let bare = tmp.join("remote.git");
        let seed = tmp.join("seed");
        fs::create_dir_all(&seed).unwrap();
        git(
            tmp,
            &[
                "init",
                "--quiet",
                "--bare",
                "-b",
                "main",
                bare.to_str().unwrap(),
            ],
        );
        git(&seed, &["init", "--quiet", "-b", "main"]);
        git(&seed, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
        git(&seed, &["push", "--quiet", bare.to_str().unwrap(), "main"]);
        bare
    }

    #[test]
    fn clones_then_updates_then_skips_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let url = bare.to_str().unwrap();
        let dest = tmp.path().join("base/host/user/repo");

        assert_eq!(sync_repo(&dest, url, "user/repo"), Outcome::Cloned);
        assert!(dest.join(".git").is_dir());
        assert_eq!(sync_repo(&dest, url, "user/repo"), Outcome::Updated);

        fs::write(dest.join("dirty.txt"), "x").unwrap();
        assert_eq!(sync_repo(&dest, url, "user/repo"), Outcome::Skipped);
    }

    #[test]
    fn bad_url_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("fail");
        let missing = tmp.path().join("nonexistent.git");
        assert_eq!(
            sync_repo(&dest, missing.to_str().unwrap(), "u/r"),
            Outcome::Errored
        );
    }

    #[test]
    fn lock_is_exclusive_and_released() {
        let tmp = tempfile::tempdir().unwrap();
        let lock = HostLock::acquire(tmp.path()).unwrap();
        assert!(HostLock::acquire(tmp.path()).is_err());
        drop(lock);
        assert!(HostLock::acquire(tmp.path()).is_ok());
    }

    #[test]
    fn prune_removes_unsynced_only() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let host = base.join("host");
        let keep = host.join("u/keep");
        let gone = host.join("u/gone");
        for p in [&keep, &gone] {
            fs::create_dir_all(p.join(".git")).unwrap();
        }
        let synced: HashSet<PathBuf> = [keep.clone()].into();
        prune(&base, &host, &synced, true).unwrap();
        assert!(gone.exists());
        prune(&base, &host, &synced, false).unwrap();
        assert!(keep.exists());
        assert!(!gone.exists());
    }

    #[test]
    fn prune_refuses_base_itself() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(prune(tmp.path(), tmp.path(), &HashSet::new(), true).is_err());
    }

    #[test]
    fn repo_path_rejects_traversal() {
        let p = repo_path(Path::new("/b/h"), "../../etc/x");
        assert_eq!(p, Path::new("/b/h").join("etc").join("x"));
    }
}
