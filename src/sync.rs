use std::collections::HashSet;
use std::fs::{self, File, TryLockError};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::thread;

use anyhow::{Result, bail};
use walkdir::WalkDir;

use crate::config::Settings;
use crate::git;
use crate::matcher::{Decision, RepoFilter};
use crate::output;
use crate::providers::{ListError, Provider, Repo};

#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    pub filter: Option<String>,
    pub prune: bool,
    pub dry_run: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub cloned: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errored: usize,
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

/// Repo names come from a remote API: on Windows a `\\` separator or a `C:`
/// drive segment would otherwise place a clone outside the base directory.
fn repo_path(host_dir: &Path, full_name: &str) -> PathBuf {
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

pub fn sync_repo(local_path: &Path, clone_url: &str) -> (Outcome, String) {
    if local_path.join(".git").is_dir() {
        if git::is_dirty(local_path) {
            return (Outcome::Skipped, "dirty, skipped".into());
        }
        update(local_path).unwrap_or_else(|e| (Outcome::Errored, format!("update failed: {e}")))
    } else {
        let existed = local_path.exists();
        let res = clone(local_path, clone_url);
        // A half-done clone would look dirty and be skipped on every later run.
        if !matches!(res, Ok(None)) && !existed {
            let _ = fs::remove_dir_all(local_path);
        }
        match res {
            Ok(None) => (Outcome::Cloned, "cloned".into()),
            Ok(Some(bad)) => (
                Outcome::Skipped,
                format!("cannot be checked out on Windows, invalid file name: {bad}"),
            ),
            Err(e) => (Outcome::Errored, format!("clone failed: {e}")),
        }
    }
}

/// Only the default branch is fast-forwarded: a sync must never move a work branch.
fn update(dir: &Path) -> Result<(Outcome, String)> {
    // --prune drops remote-tracking refs of deleted branches, so a renamed
    // default branch leaves origin/HEAD dangling and gets re-detected below.
    git::run(dir, &["fetch", "--all", "--prune", "--quiet"])?;
    if git::run(dir, &["for-each-ref", "--count=1", "refs/remotes/origin"])?.is_empty() {
        return Ok((Outcome::Skipped, "empty repository".into()));
    }
    let remote_head = || {
        git::run(
            dir,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "--abbrev-ref",
                "origin/HEAD",
            ],
        )
        .ok()
        .filter(|r| !r.is_empty() && r != "origin/HEAD")
    };
    let default = match remote_head() {
        Some(r) => r,
        None => {
            git::run(dir, &["remote", "set-head", "origin", "--auto"])?;
            remote_head().ok_or_else(|| anyhow::anyhow!("cannot find the remote default branch"))?
        }
    };
    let branch = default.strip_prefix("origin/").unwrap_or(&default);
    match git::run(dir, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        Ok(current) if current == branch => {}
        Ok(current) => return Ok((Outcome::Updated, format!("fetched, on branch {current}"))),
        Err(_) => return Ok((Outcome::Updated, "fetched, detached HEAD".into())),
    }
    let before = git::run(dir, &["rev-parse", "HEAD"]).ok();
    git::run(dir, &["merge", "--ff-only", "--quiet", &default])?;
    let after = git::run(dir, &["rev-parse", "HEAD"]).ok();
    let detail = if before == after {
        "up to date"
    } else {
        "updated"
    };
    Ok((Outcome::Updated, detail.into()))
}

/// `Some(path)`: the tree holds a path Windows cannot check out.
fn clone(local_path: &Path, url: &str) -> Result<Option<String>> {
    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let parent = local_path.parent().unwrap_or(Path::new("."));
    let target = local_path.to_string_lossy();
    if !cfg!(windows) {
        git::run(parent, &["clone", "--quiet", url, &target])?;
        return Ok(None);
    }
    // Git for Windows refuses names such as `a"b` or `CON` (core.protectNTFS),
    // which aborts a plain clone halfway. Check the tree before checking out.
    git::run(parent, &["clone", "--quiet", "--no-checkout", url, &target])?;
    let Ok(files) = git::run(local_path, &["ls-tree", "-r", "-z", "--name-only", "HEAD"]) else {
        return Ok(None); // empty remote, nothing to check out
    };
    if let Some(bad) = files.split('\0').find(|p| git::invalid_on_windows(p)) {
        return Ok(Some(bad.to_string()));
    }
    git::run(local_path, &["reset", "--quiet", "--hard", "HEAD"])?;
    Ok(None)
}

struct Job {
    host: usize,
    path: PathBuf,
    url: String,
    name: String,
}

fn pool<T: Send, R: Send>(items: Vec<T>, parallel: usize, work: impl Fn(T) -> R + Sync) -> Vec<R> {
    let queue = Mutex::new(items.into_iter());
    let results = Mutex::new(Vec::new());
    thread::scope(|s| {
        for _ in 0..parallel {
            s.spawn(|| {
                loop {
                    let next = queue.lock().unwrap().next();
                    let Some(item) = next else { break };
                    let r = work(item);
                    results.lock().unwrap().push(r);
                }
            });
        }
    });
    results.into_inner().unwrap()
}

fn run_parallel(jobs: Vec<Job>, parallel: usize) -> Vec<(usize, Outcome)> {
    let progress = output::Progress::new(jobs.len());
    pool(jobs, parallel, |job| {
        progress.start(&job.name);
        let (outcome, detail) = sync_repo(&job.path, &job.url);
        progress.finish(&job.name, |counter| {
            let msg = format!("{counter} {} ({detail})", job.name);
            match outcome {
                Outcome::Cloned | Outcome::Updated => output::success(&msg),
                Outcome::Skipped => output::warn(&msg),
                Outcome::Errored => output::error(&msg),
            }
        });
        (job.host, outcome)
    })
}

struct Listed {
    dir: PathBuf,
    repos: Vec<Repo>,
    /// Set when several hosts sync together: (heading, host).
    label: Option<(String, String)>,
}

/// Hosts list concurrently, then all their jobs share one queue so `parallel`
/// bounds the total number of git processes.
pub fn sync_hosts(targets: &[(Provider, String)], settings: &Settings, opts: &SyncOptions) -> bool {
    let multi = targets.len() > 1;
    let listings: Vec<_> = thread::scope(|s| {
        let handles: Vec<_> = targets
            .iter()
            .map(|(provider, host)| {
                s.spawn(move || -> Result<_> {
                    let dir = host_dir(&settings.base_dir, host);
                    let lock = HostLock::acquire(&dir)?;
                    output::info(&format!("Fetching repository list from {host}..."));
                    Ok((lock, dir, provider.list_repos(host)))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut ok = true;
    // Locks stay held until every job of every host is done.
    let mut locks = Vec::new();
    let mut hosts = Vec::new();
    for ((provider, host), res) in targets.iter().zip(listings) {
        let heading = format!("--- {provider} @ {host} ---");
        match res {
            Ok((lock, dir, Ok(repos))) => {
                locks.push(lock);
                hosts.push(Listed {
                    dir,
                    repos,
                    label: multi.then(|| (heading, host.clone())),
                });
            }
            Ok((_, _, Err(ListError::Skip(reason)))) => {
                if multi {
                    output::heading(&heading);
                }
                output::warn(&format!("Skipping {host}: {reason}"));
            }
            Ok((_, _, Err(ListError::Fail(e)))) | Err(e) => {
                if multi {
                    output::heading(&heading);
                }
                output::error(&format!("{host}: {e:#}"));
                ok = false;
            }
        }
    }

    for res in sync_listed(&hosts, settings, opts) {
        if let Err(e) = res {
            output::error(&format!("{e:#}"));
            ok = false;
        }
    }
    drop(locks);
    ok
}

/// Everything after listing: filter, clone/update, prune, report.
fn sync_listed(hosts: &[Listed], settings: &Settings, opts: &SyncOptions) -> Vec<Result<Summary>> {
    let filter = RepoFilter::load(&settings.base_dir, opts.filter.clone());
    let mut jobs = Vec::new();
    // Per host: paths kept by this sync (for prune) and repos skipped by the filter.
    let mut plans: Vec<(HashSet<PathBuf>, usize)> = Vec::new();

    for (i, h) in hosts.iter().enumerate() {
        if let Some((heading, _)) = &h.label {
            output::heading(heading);
        }
        output::info(&format!("Found {} repositories", h.repos.len()));
        output::blank();
        let mut synced = HashSet::new();
        let mut skipped = 0;
        for repo in &h.repos {
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
            let path = repo_path(&h.dir, full_name);
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
            jobs.push(Job {
                host: i,
                path,
                url: repo.clone_url(settings.use_https).to_string(),
                name: match &h.label {
                    Some((_, host)) => format!("{host}/{full_name}"),
                    None => full_name.clone(),
                },
            });
        }
        plans.push((synced, skipped));
    }

    let multi = hosts.len() > 1;
    if multi && !jobs.is_empty() {
        output::heading("--- sync ---");
    }
    let outcomes = run_parallel(jobs, settings.parallel);
    if multi {
        output::heading("--- summary ---");
    }

    hosts
        .iter()
        .zip(plans)
        .enumerate()
        .map(|(i, (h, (synced, skipped)))| {
            let count = |o: Outcome| outcomes.iter().filter(|x| **x == (i, o)).count();
            if opts.prune {
                if opts.filter.is_some() {
                    output::warn("Pruning skipped: not supported with --filter");
                } else {
                    prune(&settings.base_dir, &h.dir, &synced, opts.dry_run)?;
                }
            }
            let summary = Summary {
                cloned: count(Outcome::Cloned),
                updated: count(Outcome::Updated),
                skipped: count(Outcome::Skipped) + skipped,
                errored: count(Outcome::Errored),
            };
            let label = match &h.label {
                Some((_, host)) => format!(" {host}"),
                None => {
                    output::blank();
                    String::new()
                }
            };
            output::info(&format!(
                "Done{label}: {} cloned, {} updated, {} skipped, {} errors",
                summary.cloned, summary.updated, summary.skipped, summary.errored,
            ));
            Ok(summary)
        })
        .collect()
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

        assert_eq!(sync_repo(&dest, url).0, Outcome::Cloned);
        assert!(dest.join(".git").is_dir());
        assert_eq!(sync_repo(&dest, url).0, Outcome::Updated);

        fs::write(dest.join("dirty.txt"), "x").unwrap();
        assert_eq!(sync_repo(&dest, url).0, Outcome::Skipped);
    }

    #[test]
    fn pool_runs_concurrently_within_the_limit() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;
        for parallel in [1, 3] {
            let (now, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
            let out = pool((0..12).collect(), parallel, |i: i32| {
                let n = now.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(n, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(20));
                now.fetch_sub(1, Ordering::SeqCst);
                i
            });
            let mut out = out;
            out.sort();
            assert_eq!(out, (0..12).collect::<Vec<_>>());
            assert_eq!(peak.load(Ordering::SeqCst), parallel, "parallel={parallel}");
        }
    }

    #[test]
    fn empty_remote_is_skipped_not_errored() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("empty.git");
        git(
            tmp.path(),
            &["init", "--quiet", "--bare", bare.to_str().unwrap()],
        );
        let dest = tmp.path().join("clone");
        let url = bare.to_str().unwrap();
        assert_eq!(sync_repo(&dest, url).0, Outcome::Cloned);
        assert_eq!(
            sync_repo(&dest, url),
            (Outcome::Skipped, "empty repository".into())
        );
    }

    /// Paths go straight into the index: Windows could not create them on disk.
    fn remote_with_paths(tmp: &Path, names: &[&str]) -> PathBuf {
        let bare = tmp.join("odd.git");
        let seed = tmp.join("odd-seed");
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
        fs::write(seed.join("blob"), "x").unwrap();
        let out = Command::new("git")
            .arg("-C")
            .arg(&seed)
            .args(["hash-object", "-w", "blob"])
            .output()
            .unwrap();
        let sha = String::from_utf8(out.stdout).unwrap().trim().to_string();
        for n in names {
            git(
                &seed,
                &[
                    "-c",
                    "core.protectNTFS=false",
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    &format!("100644,{sha},{n}"),
                ],
            );
        }
        git(&seed, &["commit", "--quiet", "-m", "odd"]);
        git(&seed, &["push", "--quiet", bare.to_str().unwrap(), "main"]);
        bare
    }

    #[test]
    fn windows_invalid_names_skip_the_repo_cleanly() {
        let tmp = tempfile::tempdir().unwrap();
        let bad = r#"web/"Geneva": Renew.md"#;
        let bare = remote_with_paths(tmp.path(), &["ok.md", bad]);
        let dest = tmp.path().join("clone");

        let (outcome, detail) = sync_repo(&dest, bare.to_str().unwrap());
        if cfg!(windows) {
            assert_eq!(outcome, Outcome::Skipped, "{detail}");
            assert!(detail.contains(bad), "{detail}");
            assert!(!dest.exists(), "no half-done clone left behind");
        } else {
            assert_eq!(outcome, Outcome::Cloned, "{detail}");
        }
    }

    #[test]
    fn windows_checkout_of_a_valid_tree_is_clean() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = remote_with_paths(tmp.path(), &["ok.md", "web/fine.md"]);
        let dest = tmp.path().join("clone");
        let url = bare.to_str().unwrap();
        assert_eq!(sync_repo(&dest, url).0, Outcome::Cloned);
        assert!(dest.join("web").join("fine.md").is_file());
        assert!(!git::is_dirty(&dest));
        assert_eq!(sync_repo(&dest, url).0, Outcome::Updated);
    }

    #[test]
    fn failed_clone_leaves_no_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("gone");
        let missing = tmp.path().join("missing.git");
        assert_eq!(
            sync_repo(&dest, missing.to_str().unwrap()).0,
            Outcome::Errored
        );
        assert!(!dest.exists());
    }

    fn push_commit(tmp: &Path, bare: &Path, branch: &str) {
        let work = tmp.join(format!("pusher-{branch}"));
        if !work.exists() {
            git(
                tmp,
                &[
                    "clone",
                    "--quiet",
                    bare.to_str().unwrap(),
                    work.to_str().unwrap(),
                ],
            );
        }
        git(&work, &["fetch", "--quiet", "origin"]);
        git(&work, &["checkout", "--quiet", "-B", branch, "origin/main"]);
        git(&work, &["commit", "--quiet", "--allow-empty", "-m", "more"]);
        git(
            &work,
            &["push", "--quiet", "origin", &format!("HEAD:{branch}")],
        );
    }

    fn head(dir: &Path) -> String {
        git::run(dir, &["rev-parse", "HEAD"]).unwrap()
    }

    #[test]
    fn update_fast_forwards_only_the_default_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let url = bare.to_str().unwrap();
        let dest = tmp.path().join("clone");
        sync_repo(&dest, url);
        assert_eq!(
            sync_repo(&dest, url),
            (Outcome::Updated, "up to date".into())
        );

        push_commit(tmp.path(), &bare, "main");
        assert_eq!(sync_repo(&dest, url), (Outcome::Updated, "updated".into()));
        assert_eq!(
            head(&dest),
            git::run(&dest, &["rev-parse", "origin/main"]).unwrap()
        );

        // A work branch that main could fast-forward must stay where it is.
        git(&dest, &["checkout", "--quiet", "-b", "work"]);
        let work = head(&dest);
        push_commit(tmp.path(), &bare, "main");
        assert_eq!(
            sync_repo(&dest, url),
            (Outcome::Updated, "fetched, on branch work".into())
        );
        assert_eq!(head(&dest), work);

        git(&dest, &["checkout", "--quiet", "--detach"]);
        assert_eq!(
            sync_repo(&dest, url),
            (Outcome::Updated, "fetched, detached HEAD".into())
        );
        assert_eq!(head(&dest), work);
    }

    #[test]
    fn update_follows_a_renamed_default_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let url = bare.to_str().unwrap();
        let dest = tmp.path().join("clone");
        sync_repo(&dest, url);
        git(&dest, &["remote", "set-head", "origin", "--delete"]);
        assert_eq!(
            sync_repo(&dest, url).1,
            "up to date",
            "origin/HEAD is re-detected"
        );

        git(&bare, &["branch", "-m", "main", "trunk"]);
        git(&bare, &["symbolic-ref", "HEAD", "refs/heads/trunk"]);
        assert_eq!(
            sync_repo(&dest, url),
            (Outcome::Updated, "fetched, on branch main".into())
        );
        assert_eq!(
            git::run(&dest, &["rev-parse", "--abbrev-ref", "origin/HEAD"]).unwrap(),
            "origin/trunk"
        );
    }

    #[test]
    fn bad_url_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("fail");
        let missing = tmp.path().join("nonexistent.git");
        assert_eq!(
            sync_repo(&dest, missing.to_str().unwrap()).0,
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

    fn repo(name: &str, url: &Path) -> Repo {
        let url = url.to_str().unwrap().to_string();
        Repo {
            full_name: name.into(),
            ssh_url: url.clone(),
            https_url: url,
        }
    }

    fn listed(dir: &Path, repos: Vec<Repo>) -> Listed {
        Listed {
            dir: dir.to_path_buf(),
            repos,
            label: None,
        }
    }

    fn sync_one(dir: &Path, repos: Vec<Repo>, settings: &Settings, opts: &SyncOptions) -> Summary {
        let mut out = sync_listed(&[listed(dir, repos)], settings, opts);
        out.remove(0).unwrap()
    }

    fn summary(cloned: usize, updated: usize, skipped: usize, errored: usize) -> Summary {
        Summary {
            cloned,
            updated,
            skipped,
            errored,
        }
    }

    #[test]
    fn sync_listed_applies_filters_and_reports() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let base = tmp.path().join("base");
        let host = host_dir(&base, "example.org:3000");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join(".repos-filter"), "u/*\n").unwrap();
        fs::write(base.join(".repos-ignore"), "u/ignored # not wanted\n").unwrap();
        let mut settings = Settings::for_tests(&base);
        settings.parallel = 3;
        let repos = || {
            vec![
                repo("u/a", &bare),
                repo("u/nested/b", &bare),
                repo("u/ignored", &bare),
                repo("other/dropped", &bare),
                repo("u/bad", &tmp.path().join("missing.git")),
            ]
        };
        let opts = SyncOptions::default();

        let first = sync_one(&host, repos(), &settings, &opts);
        assert_eq!(first, summary(2, 0, 1, 1));
        assert!(
            host.join("u")
                .join("nested")
                .join("b")
                .join(".git")
                .is_dir()
        );
        assert!(!host.join("u").join("ignored").exists());
        assert!(!host.join("other").exists());

        fs::write(host.join("u").join("a").join("dirty.txt"), "x").unwrap();
        let second = sync_one(&host, repos(), &settings, &opts);
        assert_eq!(second, summary(0, 1, 2, 1));
    }

    #[test]
    fn dry_run_touches_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let base = tmp.path().join("base");
        let host = base.join("h");
        let existing = host.join("u").join("old");
        let stale = host.join("u").join("stale");
        for p in [&existing, &stale] {
            fs::create_dir_all(p.join(".git")).unwrap();
        }
        let settings = Settings::for_tests(&base);
        let opts = SyncOptions {
            prune: true,
            dry_run: true,
            ..Default::default()
        };
        let repos = vec![repo("u/old", &bare), repo("u/new", &bare)];
        let s = sync_one(&host, repos, &settings, &opts);
        assert_eq!(s, Summary::default());
        assert!(!host.join("u").join("new").exists());
        assert!(stale.exists());
    }

    #[test]
    fn prune_after_sync_and_refused_with_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let base = tmp.path().join("base");
        let host = base.join("h");
        let stale = host.join("u").join("stale");
        fs::create_dir_all(stale.join(".git")).unwrap();
        let mut settings = Settings::for_tests(&base);
        settings.use_https = true;
        let repos = || {
            vec![Repo {
                ssh_url: "unused".into(),
                ..repo("u/kept", &bare)
            }]
        };

        let filtered = SyncOptions {
            filter: Some("u/*".into()),
            prune: true,
            dry_run: false,
        };
        let s = sync_one(&host, repos(), &settings, &filtered);
        assert_eq!(s, summary(1, 0, 0, 0));
        assert!(stale.exists(), "prune is skipped with --filter");

        let opts = SyncOptions {
            prune: true,
            ..Default::default()
        };
        let s = sync_one(&host, repos(), &settings, &opts);
        assert_eq!(s, summary(0, 1, 0, 0));
        assert!(!stale.exists());
        assert!(host.join("u").join("kept").exists());
    }

    #[test]
    fn sync_hosts_refuses_a_locked_host() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = Settings::for_tests(tmp.path());
        let _lock = HostLock::acquire(&host_dir(tmp.path(), "github.com")).unwrap();
        let targets = [(Provider::Github, "github.com".to_string())];
        assert!(!sync_hosts(&targets, &settings, &SyncOptions::default()));
    }

    #[test]
    fn hosts_share_one_queue_and_report_separately() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let base = tmp.path().join("base");
        let settings = Settings::for_tests(&base);
        let (a, b) = (base.join("a"), base.join("b"));
        let hosts = [
            Listed {
                label: Some(("a".into(), "a".into())),
                ..listed(&a, vec![repo("u/x", &bare), repo("u/y", &bare)])
            },
            Listed {
                label: Some(("b".into(), "b".into())),
                ..listed(
                    &b,
                    vec![
                        repo("u/x", &bare),
                        repo("u/bad", &tmp.path().join("no.git")),
                    ],
                )
            },
        ];
        let out: Vec<_> = sync_listed(&hosts, &settings, &SyncOptions::default())
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(out, [summary(2, 0, 0, 0), summary(1, 0, 0, 1)]);
        assert!(a.join("u").join("y").join(".git").is_dir());
        assert!(b.join("u").join("x").join(".git").is_dir());
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
    fn prune_edge_cases() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        fs::create_dir_all(&base).unwrap();
        prune(&base, &base.join("missing"), &HashSet::new(), false).unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        assert!(prune(&base, &outside, &HashSet::new(), true).is_err());
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
