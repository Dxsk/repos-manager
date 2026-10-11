//! Scheduling the clone and update jobs, and the git work each one does.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use anyhow::{Result, bail};

use super::Outcome;
use crate::git;
use crate::output;

pub fn sync_repo(local_path: &Path, clone_url: &str) -> (Outcome, String) {
    if git::is_clone(local_path) {
        if git::is_dirty(local_path) {
            return (Outcome::Skipped, "dirty, skipped".into());
        }
        update(local_path).unwrap_or_else(|e| (Outcome::Errored, format!("update failed: {e}")))
    } else {
        match clone(local_path, clone_url) {
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

/// The clone lands in a `.<name>.partial` sibling and is renamed once
/// complete, so an interrupted run never leaves a half-done repo behind.
/// `Some(path)`: the tree holds a path Windows cannot check out.
fn clone(local_path: &Path, url: &str) -> Result<Option<String>> {
    let parent = local_path.parent().unwrap_or(Path::new("."));
    let name = local_path.file_name().unwrap_or_default().to_string_lossy();
    let partial = parent.join(format!(".{name}.partial"));
    if partial.exists() {
        fs::remove_dir_all(&partial)?;
    }
    fs::create_dir_all(parent)?;
    let res = clone_into(&partial, url).and_then(|bad| {
        if bad.is_none() {
            if local_path.is_dir() && fs::read_dir(local_path)?.next().is_none() {
                fs::remove_dir(local_path)?;
            }
            rename_dir(&partial, local_path)?;
        }
        Ok(bad)
    });
    if partial.exists() {
        let _ = fs::remove_dir_all(&partial);
    }
    res
}

fn clone_into(dir: &Path, url: &str) -> Result<Option<String>> {
    let parent = dir.parent().unwrap_or(Path::new("."));
    let target = dir.to_string_lossy();
    if !cfg!(windows) {
        git::run(parent, &["clone", "--quiet", url, &target])?;
        return Ok(None);
    }
    // Git for Windows refuses names such as `a"b` or `CON` (core.protectNTFS),
    // which aborts a plain clone halfway. Check the tree before checking out.
    git::run(parent, &["clone", "--quiet", "--no-checkout", url, &target])?;
    let Ok(files) = git::run(dir, &["ls-tree", "-r", "-z", "--name-only", "HEAD"]) else {
        return Ok(None); // empty remote, nothing to check out
    };
    if let Some(bad) = files.split('\0').find(|p| git::invalid_on_windows(p)) {
        return Ok(Some(bad.to_string()));
    }
    git::run(dir, &["reset", "--quiet", "--hard", "HEAD"])?;
    Ok(None)
}

const RENAME_ATTEMPTS: u32 = 5;

/// How long to wait before renaming again, or `None` to give up. Antivirus
/// scanners briefly hold fresh files open on Windows, so a rename right after a
/// clone fails with access denied. Nowhere else needs it, and an occupied
/// destination can only fail again. `windows` is a parameter to test both policies.
fn rename_backoff(attempt: u32, to: &Path, windows: bool) -> Option<Duration> {
    (windows && attempt < RENAME_ATTEMPTS && !to.exists())
        .then(|| Duration::from_millis(100 * u64::from(attempt + 1)))
}

fn rename_dir(from: &Path, to: &Path) -> Result<()> {
    for attempt in 0..=RENAME_ATTEMPTS {
        let Err(e) = fs::rename(from, to) else {
            return Ok(());
        };
        let Some(wait) = rename_backoff(attempt, to, cfg!(windows)) else {
            bail!("cannot move clone into place: {e}");
        };
        output::debug(&format!("rename {}: {e}, retrying", output::path(from)));
        thread::sleep(wait);
    }
    bail!("cannot move {} into place", output::path(from))
}

pub(super) struct Job {
    pub(super) host: usize,
    /// Clones before updates, biggest clone first.
    pub(super) weight: u64,
    pub(super) path: PathBuf,
    pub(super) url: String,
    pub(super) name: String,
}

/// Jobs per host, from sshd's default `MaxStartups` (`10:30:100`): once 10
/// connections are authenticating, sshd refuses new ones at random. This bounds
/// our share of a server-wide count, it does not rule out a refusal.
const PER_HOST: usize = 10;

/// Run `work` on at most `parallel` threads, never more than `cap` per group.
/// A free worker serves the group with the fewest running, then the one whose
/// next item is heaviest: spreading spares any single server, and a big job
/// started last sets the makespan. A heuristic modelled on LPT, with no bound.
fn schedule<T: Send, R: Send>(
    items: Vec<T>,
    parallel: usize,
    cap: usize,
    key: impl Fn(&T) -> (usize, u64),
    work: impl Fn(T) -> R + Sync,
) -> Vec<R> {
    let groups = items.iter().map(|i| key(i).0 + 1).max().unwrap_or(0);
    let mut queues: Vec<Vec<(u64, T)>> = (0..groups).map(|_| Vec::new()).collect();
    for item in items {
        let (g, w) = key(&item);
        queues[g].push((w, item));
    }
    for q in &mut queues {
        // Stable, so equal weights keep their listing order.
        q.sort_by_key(|(w, _)| std::cmp::Reverse(*w));
        q.reverse();
    }
    let state = Mutex::new((queues, vec![0usize; groups]));
    let freed = std::sync::Condvar::new();
    let results = Mutex::new(Vec::new());
    thread::scope(|s| {
        for _ in 0..parallel {
            s.spawn(|| {
                loop {
                    let mut st = state.lock().unwrap();
                    let (g, item) = loop {
                        let (queues, running) = &mut *st;
                        let pick = (0..queues.len())
                            .filter(|&g| !queues[g].is_empty() && running[g] < cap)
                            .min_by_key(|&g| {
                                let head = queues[g].last().map_or(0, |(w, _)| *w);
                                (running[g], std::cmp::Reverse(head))
                            });
                        if let Some(g) = pick {
                            running[g] += 1;
                            break (g, queues[g].pop().unwrap().1);
                        }
                        if queues.iter().all(Vec::is_empty) {
                            return;
                        }
                        st = freed.wait(st).unwrap();
                    };
                    drop(st);
                    let r = work(item);
                    results.lock().unwrap().push(r);
                    state.lock().unwrap().1[g] -= 1;
                    freed.notify_all();
                }
            });
        }
    });
    results.into_inner().unwrap()
}

pub(super) fn run_parallel(jobs: Vec<Job>, parallel: usize) -> Vec<(usize, Outcome)> {
    let progress = output::Progress::new(jobs.len());
    schedule(
        jobs,
        parallel,
        PER_HOST,
        |j| (j.host, j.weight),
        |job| {
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
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::tests::{bare_remote, git};
    use std::process::Command;
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
            let out = schedule(
                (0..12).collect(),
                parallel,
                usize::MAX,
                |_| (0, 0),
                |i: i32| {
                    let n = now.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(n, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(20));
                    now.fetch_sub(1, Ordering::SeqCst);
                    i
                },
            );
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
    fn clone_replaces_leftovers_of_an_interrupted_run() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let dest = tmp.path().join("u").join("repo");
        let partial = tmp.path().join("u").join(".repo.partial");
        fs::create_dir_all(partial.join(".git")).unwrap();
        fs::write(partial.join("junk"), "x").unwrap();
        fs::create_dir_all(&dest).unwrap();

        assert_eq!(sync_repo(&dest, bare.to_str().unwrap()).0, Outcome::Cloned);
        assert!(!partial.exists());
        assert!(!git::is_dirty(&dest));
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
        assert!(!tmp.path().join(".gone.partial").exists());
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
    fn schedule_runs_heaviest_first_across_hosts() {
        // (host, weight)
        let items = vec![(0, 1), (0, 5), (0, 3), (1, 2), (1, 9)];
        let order = schedule(items, 1, usize::MAX, |&i| i, |i| i);
        assert_eq!(order, [(1, 9), (0, 5), (0, 3), (1, 2), (0, 1)]);
    }

    #[test]
    fn schedule_caps_each_host() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;
        let now = [AtomicUsize::new(0), AtomicUsize::new(0)];
        let peak = [AtomicUsize::new(0), AtomicUsize::new(0)];
        let items: Vec<usize> = (0..24).map(|i| i % 2).collect();
        schedule(
            items,
            8,
            3,
            |&g| (g, 0),
            |g| {
                let n = now[g].fetch_add(1, Ordering::SeqCst) + 1;
                peak[g].fetch_max(n, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(15));
                now[g].fetch_sub(1, Ordering::SeqCst);
            },
        );
        for p in &peak {
            assert_eq!(p.load(Ordering::SeqCst), 3);
        }
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
    fn rename_retries_only_while_the_destination_is_free() {
        let tmp = tempfile::tempdir().unwrap();
        let free = tmp.path().join("free");
        let taken = tmp.path().join("taken");
        fs::create_dir(&taken).unwrap();

        assert!(rename_backoff(0, &free, true).is_some());
        assert!(rename_backoff(0, &free, false).is_none());
        // A destination already in use will not free itself.
        assert!(rename_backoff(0, &taken, true).is_none());
        assert!(rename_backoff(RENAME_ATTEMPTS - 1, &free, true).is_some());
        assert!(rename_backoff(RENAME_ATTEMPTS, &free, true).is_none());
        let waits: Vec<_> = (0..RENAME_ATTEMPTS)
            .map(|a| rename_backoff(a, &free, true).unwrap())
            .collect();
        assert!(waits.windows(2).all(|w| w[0] < w[1]), "{waits:?}");
        assert_eq!(waits[0], Duration::from_millis(100));

        // A rename that can never work is still reported.
        let err = rename_dir(&tmp.path().join("missing"), &free).unwrap_err();
        assert!(
            err.to_string().contains("cannot move clone into place"),
            "{err}"
        );
    }
}

#[cfg(test)]
mod properties {
    use super::*;
    use proptest::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    proptest! {
        #[test]
        fn schedule_respects_both_limits(
            groups in prop::collection::vec(0usize..4, 0..40),
            parallel in 1usize..8,
            cap in 1usize..4,
        ) {
            let now: Vec<AtomicUsize> = (0..4).map(|_| AtomicUsize::new(0)).collect();
            let peak: Vec<AtomicUsize> = (0..4).map(|_| AtomicUsize::new(0)).collect();
            let total = AtomicUsize::new(0);
            let total_peak = AtomicUsize::new(0);
            let items: Vec<(usize, usize)> = groups.iter().copied().enumerate().collect();
            let mut out = schedule(items, parallel, cap, |&(i, g)| (g, i as u64), |(i, g)| {
                let n = now[g].fetch_add(1, Ordering::SeqCst) + 1;
                peak[g].fetch_max(n, Ordering::SeqCst);
                let t = total.fetch_add(1, Ordering::SeqCst) + 1;
                total_peak.fetch_max(t, Ordering::SeqCst);
                std::thread::yield_now();
                total.fetch_sub(1, Ordering::SeqCst);
                now[g].fetch_sub(1, Ordering::SeqCst);
                i
            });
            out.sort();
            prop_assert_eq!(out, (0..groups.len()).collect::<Vec<_>>());
            prop_assert!(total_peak.load(Ordering::SeqCst) <= parallel);
            for p in &peak {
                prop_assert!(p.load(Ordering::SeqCst) <= cap);
            }
        }

        #[test]
        fn pool_returns_every_result_within_the_limit(n in 0usize..40, parallel in 1usize..8) {
            let (now, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
            let mut out = schedule((0..n).collect(), parallel, usize::MAX, |_| (0, 0), |i| {
                let c = now.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(c, Ordering::SeqCst);
                std::thread::yield_now();
                now.fetch_sub(1, Ordering::SeqCst);
                i
            });
            out.sort();
            prop_assert_eq!(out, (0..n).collect::<Vec<_>>());
            prop_assert!(peak.load(Ordering::SeqCst) <= parallel);
        }
    }
}
