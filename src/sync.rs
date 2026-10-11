use std::collections::{HashMap, HashSet};
use std::fs::{self, File, TryLockError};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use anyhow::{Result, bail};
use unicode_normalization::UnicodeNormalization;
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
    pub force: bool,
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
/// scanners briefly hold freshly written files open on Windows, which makes a
/// rename right after a clone fail with access denied. Nowhere else needs it,
/// and once something occupies the destination retrying can only fail again.
/// `windows` is a parameter so both policies are testable on either platform.
fn rename_backoff(attempt: u32, to: &Path, windows: bool) -> Option<Duration> {
    (windows && attempt < RENAME_ATTEMPTS && !to.exists())
        .then(|| Duration::from_millis(100 * u64::from(attempt + 1)))
}

fn rename_dir(from: &Path, to: &Path) -> Result<()> {
    // The range bounds the retries, so no counter can leave this spinning.
    for attempt in 0..=RENAME_ATTEMPTS {
        let Err(e) = fs::rename(from, to) else {
            return Ok(());
        };
        let Some(wait) = rename_backoff(attempt, to, cfg!(windows)) else {
            bail!("cannot move clone into place: {e}");
        };
        output::debug(&format!("rename {}: {e}, retrying", from.display()));
        thread::sleep(wait);
    }
    // Unreachable: the last attempt gets no backoff and bails above.
    bail!("cannot move {} into place", from.display())
}

struct Job {
    host: usize,
    /// Clones before updates, biggest clone first.
    weight: u64,
    path: PathBuf,
    url: String,
    name: String,
}

/// Jobs per host, taken from sshd's default `MaxStartups` (`10:30:100`): once
/// 10 connections are still authenticating, sshd refuses new ones at random.
/// A job starts only as another ends, so fewer than 10 of ours are ever open
/// when a new one arrives. That count is server-wide though, and every other
/// client adds to it: this bounds our share, it does not rule out a refusal.
const PER_HOST: usize = 10;

/// Run `work` on at most `parallel` threads, never more than `cap` per group.
/// A free worker serves the group with the fewest running, then the one whose
/// next item is heaviest: spreading spares any single server, and heaviest
/// first shortens the tail, since a big job started last sets the makespan.
/// LPT's 4/3 bound does not carry over: it assumes known durations, identical
/// machines and no per-group cap, while the repo size only stands in for the
/// duration here. A heuristic motivated by LPT, not a guarantee.
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

fn run_parallel(jobs: Vec<Job>, parallel: usize) -> Vec<(usize, Outcome)> {
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

struct Listed {
    dir: PathBuf,
    repos: Vec<Repo>,
    /// Set when several hosts sync together: (heading, host).
    label: Option<(String, String)>,
    /// False when a provider sharing this directory failed or was skipped:
    /// its repos are missing from `repos`, so pruning would delete them.
    complete: bool,
    announced: bool,
}

struct Group<'a> {
    key: String,
    dir: PathBuf,
    members: Vec<(Provider, &'a str)>,
}

/// Hosts list concurrently, then all their jobs share one queue so `parallel`
/// bounds the total number of git processes.
/// The host directory, not the (provider, host) pair, is what the lock and
/// prune act on, so targets sharing one are listed together under one lock.
/// Hostnames are case-insensitive, and so is the Windows filesystem. The same
/// pair twice is one target; the same host under two providers is two.
fn group_targets<'a>(targets: &'a [(Provider, String)], base: &Path) -> Vec<Group<'a>> {
    let mut groups: Vec<Group> = Vec::new();
    for (provider, host) in targets {
        let dir = host_dir(base, host);
        let key = dir.to_string_lossy().to_lowercase();
        let member = (*provider, host.as_str());
        match groups.iter_mut().find(|g| g.key == key) {
            Some(g) if g.members.contains(&member) => {}
            Some(g) => g.members.push(member),
            None => groups.push(Group {
                key,
                dir,
                members: vec![member],
            }),
        }
    }
    groups
}

/// Providers sharing a host directory list into it together, so a project
/// both of them report is one repository. The first listing wins, which keeps
/// the order providers were configured in.
fn merge_unique(into: &mut Vec<Repo>, listed: Vec<Repo>) {
    for repo in listed {
        if !into.iter().any(|x| x.full_name == repo.full_name) {
            into.push(repo);
        }
    }
}

pub fn sync_hosts(targets: &[(Provider, String)], settings: &Settings, opts: &SyncOptions) -> bool {
    let groups = group_targets(targets, &settings.base_dir);
    let multi = groups.len() > 1;

    let listings: Vec<_> = thread::scope(|s| {
        let handles: Vec<_> = groups
            .iter()
            .map(|Group { dir, members, .. }| {
                s.spawn(move || -> Result<_> {
                    let lock = HostLock::acquire(dir)?;
                    let lists: Vec<_> = members
                        .iter()
                        .map(|(provider, host)| {
                            output::info(&format!("Fetching repository list from {host}..."));
                            (*host, provider.list_repos(host, settings))
                        })
                        .collect();
                    Ok((lock, lists))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut ok = true;
    // Locks stay held until every job of every host is done.
    let mut locks = Vec::new();
    let mut hosts = Vec::new();
    for (Group { dir, members, .. }, res) in groups.iter().zip(listings) {
        let providers: Vec<_> = members.iter().map(|(p, _)| p.to_string()).collect();
        let host = members[0].1;
        let heading = format!("--- {} @ {host} ---", providers.join("+"));
        // Printed before the first warning, else by sync_listed with the repos.
        let mut announced = false;
        let mut announce = || {
            if multi && !announced {
                output::heading(&heading);
                announced = true;
            }
        };
        let lists = match res {
            Ok((lock, lists)) => {
                locks.push(lock);
                lists
            }
            Err(e) => {
                announce();
                output::error(&format!("{host}: {e:#}"));
                ok = false;
                continue;
            }
        };
        let mut repos: Vec<Repo> = Vec::new();
        let (mut complete, mut any) = (true, false);
        for (h, list) in lists {
            match list {
                Ok(r) => {
                    any = true;
                    merge_unique(&mut repos, r);
                }
                Err(ListError::Skip(reason)) => {
                    announce();
                    output::warn(&format!("Skipping {h}: {reason}"));
                    complete = false;
                }
                Err(ListError::Fail(e)) => {
                    announce();
                    output::error(&format!("{h}: {e:#}"));
                    complete = false;
                    ok = false;
                }
            }
        }
        if any {
            hosts.push(Listed {
                dir: dir.clone(),
                repos,
                label: multi.then(|| (heading.clone(), host.to_string())),
                complete,
                announced,
            });
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
    // Per host: every listed path (never pruned) and repos skipped by the filter.
    let mut plans: Vec<(HashSet<PathBuf>, usize)> = Vec::new();

    for (i, h) in hosts.iter().enumerate() {
        if let Some((heading, _)) = &h.label
            && !h.announced
        {
            output::heading(heading);
        }
        output::info(&format!("Found {} repositories", h.repos.len()));
        output::blank();
        let mut listed = HashSet::new();
        let mut skipped = 0;
        let mut kept = Vec::new();
        for repo in &h.repos {
            let full_name = &repo.full_name;
            let path = repo_path(&h.dir, full_name);
            listed.insert(path.clone());
            match filter.decide(full_name) {
                Decision::Drop => continue,
                Decision::Ignored => {
                    output::skip(&format!("{full_name} (ignored)"));
                    skipped += 1;
                    continue;
                }
                Decision::Keep => kept.push((repo, path)),
            }
        }
        let url = |r: &Repo| r.clone_url(settings.use_https).to_string();
        let candidates: Vec<_> = kept
            .iter()
            .map(|(r, p)| (repo_name(&h.dir, p), p.as_path(), url(r)))
            .collect();
        let conflicts = path_conflicts(&candidates, Fs::current(), origin_url);
        for (n, (repo, path)) in kept.into_iter().enumerate() {
            let full_name = &repo.full_name;
            if let Some(reason) = conflicts.get(&n) {
                output::warn(&format!("{full_name} (skipped: {reason})"));
                skipped += 1;
                continue;
            }
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
                weight: if path.join(".git").is_dir() {
                    0
                } else {
                    repo.size_kb + 1
                },
                path,
                url: url(repo),
                name: match &h.label {
                    Some((_, host)) => format!("{host}/{full_name}"),
                    None => full_name.clone(),
                },
            });
        }
        plans.push((listed, skipped));
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
        .map(|(i, (h, (listed, skipped)))| {
            let count = |o: Outcome| outcomes.iter().filter(|x| **x == (i, o)).count();
            if opts.prune && !h.complete {
                output::warn("Pruning skipped: the repository list of this host is incomplete");
            } else if opts.prune {
                prune(&settings.base_dir, &h.dir, &listed, &filter, opts)?;
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

fn is_partial_clone(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with('.') && name.ends_with(".partial")
}

/// Find git working copies under `root` without descending into `.git` itself.
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

/// Deletion is irreversible while the remote listing can be wrong (renamed
/// repo, revoked access, truncated API answer), so a candidate must be
/// unlisted, in the filter's scope and free of local-only work.
pub fn prune(
    base: &Path,
    host_dir: &Path,
    listed: &HashSet<PathBuf>,
    filter: &RepoFilter,
    opts: &SyncOptions,
) -> Result<()> {
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

    let local = find_repos(host_dir);
    let candidates: Vec<&PathBuf> = local
        .iter()
        .filter(|r| !listed.contains(*r))
        .filter(|r| matches!(filter.decide(&repo_name(host_dir, r)), Decision::Keep))
        .collect();
    if candidates.len() >= 2 && candidates.len() * 2 > local.len() && !opts.force {
        let msg = format!(
            "{} of {} local repositories are no longer listed, the listing looks incomplete",
            candidates.len(),
            local.len()
        );
        if !opts.dry_run {
            bail!("prune aborted: {msg}. Review with --dry-run, then add --force");
        }
        output::warn(&format!("{msg}: a real run needs --force"));
    }

    let mut pruned = 0;
    for repo in candidates {
        let rel = repo
            .strip_prefix(base)
            .unwrap_or(repo)
            .display()
            .to_string();
        if let Some(reason) = unsaved_work(repo) {
            output::warn(&format!("{rel} (not pruned: {reason})"));
            continue;
        }
        if opts.dry_run {
            output::warn(&format!("[dry-run] would prune {rel}"));
        } else {
            output::error(&format!("{rel} (pruned)"));
            fs::remove_dir_all(repo)?;
        }
        pruned += 1;
    }
    if pruned > 0 {
        output::warn(&format!("Pruned {pruned} repositories"));
    }
    Ok(())
}

/// When the target filesystem calls two different repo names one directory.
#[derive(Debug, Clone, Copy)]
struct Fs {
    /// Windows refuses some names outright (`aux`, a trailing dot).
    windows: bool,
    /// NTFS and APFS fold case, ext4 does not.
    case_insensitive: bool,
    /// APFS and HFS+ compare Unicode-normalized, so a precomposed `é` (NFC)
    /// and a decomposed one (NFD) are the same directory there, while NTFS
    /// and ext4 compare code points and see two.
    normalizing: bool,
}

impl Fs {
    const fn current() -> Self {
        Self {
            windows: cfg!(windows),
            case_insensitive: cfg!(any(windows, target_os = "macos")),
            normalizing: cfg!(any(target_os = "macos", target_os = "ios")),
        }
    }

    /// The directory a name claims. Two names collide when their keys match.
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
/// the losers by index. On a collision the repo already cloned on disk wins,
/// so a new remote repo never takes over an existing clone.
fn path_conflicts(
    repos: &[(String, &Path, String)],
    target: Fs,
    origin: impl Fn(&Path) -> Option<String>,
) -> HashMap<usize, String> {
    let mut losers = HashMap::new();
    // Repos grouped by the directory they would share.
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

fn origin_url(repo: &Path) -> Option<String> {
    git::run(repo, &["remote", "get-url", "origin"]).ok()
}

fn repo_name(host_dir: &Path, repo: &Path) -> String {
    let rel = repo.strip_prefix(host_dir).unwrap_or(repo);
    let parts: Vec<_> = rel.iter().map(|c| c.to_string_lossy()).collect();
    parts.join("/")
}

/// Work that exists only in this clone. A repo git cannot read counts as
/// unsafe too.
fn unsaved_work(repo: &Path) -> Option<&'static str> {
    let run = |args: &[&str]| git::run(repo, args);
    match run(&["status", "--porcelain"]) {
        Err(_) => return Some("cannot read its state"),
        Ok(s) if !s.is_empty() => return Some("uncommitted changes"),
        Ok(_) => {}
    }
    match run(&["rev-list", "-n", "1", "--branches", "--not", "--remotes"]) {
        Err(_) => return Some("cannot read its state"),
        Ok(s) if !s.is_empty() => return Some("unpushed commits"),
        Ok(_) => {}
    }
    run(&["rev-parse", "--quiet", "--verify", "refs/stash"])
        .is_ok()
        .then_some("stashed changes")
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
        // Nothing but Windows needs this.
        assert!(rename_backoff(0, &free, false).is_none());
        // A destination already in use will not free itself.
        assert!(rename_backoff(0, &taken, true).is_none());
        assert!(rename_backoff(RENAME_ATTEMPTS - 1, &free, true).is_some());
        assert!(rename_backoff(RENAME_ATTEMPTS, &free, true).is_none());
        // Each wait is longer than the last.
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

    #[test]
    fn lock_is_exclusive_and_released() {
        let tmp = tempfile::tempdir().unwrap();
        let lock = HostLock::acquire(tmp.path()).unwrap();
        let path = lock.path.clone();
        assert!(path.is_file());
        assert!(HostLock::acquire(tmp.path()).is_err());
        drop(lock);
        assert!(HostLock::acquire(tmp.path()).is_ok());
        // Nothing left behind once the last holder is gone.
        assert!(!path.exists(), "{} survived the lock", path.display());
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
            ..Default::default()
        }
    }

    fn listed(dir: &Path, repos: Vec<Repo>) -> Listed {
        Listed {
            dir: dir.to_path_buf(),
            repos,
            label: None,
            complete: true,
            announced: false,
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

    fn init_repo(p: &Path) {
        fs::create_dir_all(p).unwrap();
        git(p, &["init", "--quiet"]);
    }

    fn prune_opts(dry_run: bool, force: bool) -> SyncOptions {
        SyncOptions {
            prune: true,
            dry_run,
            force,
            ..Default::default()
        }
    }

    fn no_filter(base: &Path) -> RepoFilter {
        RepoFilter::load(base, None)
    }

    #[test]
    fn prune_removes_unlisted_only() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let host = base.join("host");
        let (keep, gone) = (host.join("u/keep"), host.join("u/gone"));
        init_repo(&keep);
        init_repo(&gone);
        let listed: HashSet<PathBuf> = [keep.clone()].into();
        prune(
            &base,
            &host,
            &listed,
            &no_filter(&base),
            &prune_opts(true, false),
        )
        .unwrap();
        assert!(gone.exists());
        prune(
            &base,
            &host,
            &listed,
            &no_filter(&base),
            &prune_opts(false, false),
        )
        .unwrap();
        assert!(keep.exists());
        assert!(!gone.exists());
    }

    #[test]
    fn prune_never_destroys_local_only_work() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let base = tmp.path().join("base");
        let host = base.join("h");
        let url = bare.to_str().unwrap();
        let clone = |name: &str| {
            let p = host.join("u").join(name);
            assert_eq!(sync_repo(&p, url).0, Outcome::Cloned);
            p
        };
        let dirty = clone("dirty");
        fs::write(dirty.join("wip.txt"), "x").unwrap();
        let unpushed = clone("unpushed");
        git(
            &unpushed,
            &["commit", "--quiet", "--allow-empty", "-m", "local"],
        );
        let stashed = clone("stashed");
        fs::write(stashed.join("wip.txt"), "x").unwrap();
        git(&stashed, &["stash", "--quiet", "--include-untracked"]);
        let local_only = host.join("u").join("local-only");
        init_repo(&local_only);
        git(
            &local_only,
            &["commit", "--quiet", "--allow-empty", "-m", "mine"],
        );
        let broken = host.join("u").join("broken");
        fs::create_dir_all(broken.join(".git")).unwrap();
        let clean = clone("clean");

        let listed = HashSet::new();
        let opts = prune_opts(false, true);
        prune(&base, &host, &listed, &no_filter(&base), &opts).unwrap();
        for p in [&dirty, &unpushed, &stashed, &local_only, &broken] {
            assert!(p.exists(), "{} must survive", p.display());
        }
        assert!(!clean.exists());
    }

    #[test]
    fn prune_leaves_ignored_and_filtered_out_repos_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let base = tmp.path().join("base");
        let host = base.join("h");
        let settings = Settings::for_tests(&base);
        let all = || {
            vec![
                repo("u/a", &bare),
                repo("u/b", &bare),
                repo("other/c", &bare),
            ]
        };
        sync_one(&host, all(), &settings, &SyncOptions::default());
        let stale = host.join("u").join("stale");
        init_repo(&stale);
        fs::write(base.join(".repos-ignore"), "u/a\n").unwrap();
        fs::write(base.join(".repos-filter"), "u/*\n").unwrap();

        // u/a is ignored and other/c filtered out, yet both are still on the remote.
        sync_one(&host, all(), &settings, &prune_opts(false, false));
        assert!(host.join("u").join("a").exists());
        assert!(host.join("other").join("c").exists());
        assert!(!stale.exists());

        // Gone from the remote but outside the filter's scope: not ours to delete.
        let opts = SyncOptions {
            filter: Some("u/*".into()),
            ..prune_opts(false, false)
        };
        sync_one(
            &host,
            vec![repo("u/a", &bare), repo("u/b", &bare)],
            &settings,
            &opts,
        );
        assert!(host.join("other").join("c").exists());
    }

    #[test]
    fn prune_refuses_a_listing_that_shrank_suspiciously() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let host = base.join("h");
        let repos: Vec<_> = ["a", "b", "c"]
            .iter()
            .map(|n| host.join("u").join(n))
            .collect();
        for r in &repos {
            init_repo(r);
        }
        let listed: HashSet<PathBuf> = [repos[0].clone()].into();
        let filter = no_filter(&base);

        let err = prune(&base, &host, &listed, &filter, &prune_opts(false, false)).unwrap_err();
        assert!(err.to_string().contains("2 of 3"), "{err}");
        prune(&base, &host, &listed, &filter, &prune_opts(true, false)).unwrap();
        assert!(repos.iter().all(|r| r.exists()));

        prune(&base, &host, &listed, &filter, &prune_opts(false, true)).unwrap();
        assert!(repos[0].exists() && !repos[1].exists() && !repos[2].exists());
    }

    #[test]
    fn prune_stops_once_more_than_half_a_host_would_go() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let filter = no_filter(&base);
        let setup = |host: &Path, n: usize, keep: usize| {
            let repos: Vec<_> = (0..n).map(|i| host.join("u").join(i.to_string())).collect();
            for r in &repos {
                init_repo(r);
            }
            let listed: HashSet<PathBuf> = repos[..keep].iter().cloned().collect();
            (repos, listed)
        };

        // Exactly half is still within reason.
        let host = base.join("half");
        let (repos, listed) = setup(&host, 4, 2);
        prune(&base, &host, &listed, &filter, &prune_opts(false, false)).unwrap();
        assert!(repos[..2].iter().all(|r| r.exists()));
        assert!(repos[2..].iter().all(|r| !r.exists()));

        // Past half, it refuses until --force.
        let host = base.join("most");
        let (repos, listed) = setup(&host, 5, 2);
        let err = prune(&base, &host, &listed, &filter, &prune_opts(false, false)).unwrap_err();
        assert!(err.to_string().contains("3 of 5"), "{err}");
        assert!(repos.iter().all(|r| r.exists()));
    }

    #[test]
    fn prune_guards_the_base_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        fs::create_dir_all(&base).unwrap();
        let (none, filter) = (HashSet::new(), no_filter(&base));
        let opts = prune_opts(false, true);
        prune(&base, &base.join("missing"), &none, &filter, &opts).unwrap();
        assert!(prune(&base, &base, &none, &filter, &opts).is_err());
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        assert!(prune(&base, &outside, &none, &filter, &opts).is_err());
    }

    #[test]
    fn partial_clones_are_not_repos() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("u").join(".r.partial").join(".git")).unwrap();
        assert!(find_repos(tmp.path()).is_empty());
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
            init_repo(p);
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
    fn sync_hosts_refuses_a_locked_host() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = Settings::for_tests(tmp.path());
        let _lock = HostLock::acquire(&host_dir(tmp.path(), "github.com")).unwrap();
        let targets = [(Provider::Github, "github.com".to_string())];
        assert!(!sync_hosts(&targets, &settings, &SyncOptions::default()));
    }

    #[test]
    fn providers_on_one_host_merge_into_one_repo_list() {
        let named = |n: &str, url: &str| Repo {
            full_name: n.into(),
            ssh_url: url.into(),
            https_url: url.into(),
            ..Default::default()
        };
        let mut repos = vec![named("u/a", "first"), named("u/b", "first")];
        merge_unique(
            &mut repos,
            vec![named("u/a", "second"), named("u/c", "second")],
        );
        let seen: Vec<_> = repos
            .iter()
            .map(|r| (r.full_name.as_str(), r.ssh_url.as_str()))
            .collect();
        // u/a keeps the first provider's url, u/c is new.
        assert_eq!(
            seen,
            [("u/a", "first"), ("u/b", "first"), ("u/c", "second")]
        );
    }

    #[test]
    fn targets_are_grouped_by_directory_without_duplicates() {
        let base = Path::new("base");
        let targets = [
            (Provider::Forgejo, "x.invalid".to_string()),
            (Provider::Forgejo, "x.invalid".to_string()),
            (Provider::Github, "x.invalid".to_string()),
            (Provider::Forgejo, "other.invalid".to_string()),
        ];
        let groups = group_targets(&targets, base);
        let members: Vec<_> = groups.iter().map(|g| g.members.as_slice()).collect();
        assert_eq!(
            members,
            [
                // The repeated pair is dropped, the second provider is not.
                &[
                    (Provider::Forgejo, "x.invalid"),
                    (Provider::Github, "x.invalid"),
                ][..],
                &[(Provider::Forgejo, "other.invalid")][..],
            ]
        );
        assert_eq!(groups[0].dir, host_dir(base, "x.invalid"));
    }

    #[test]
    fn targets_sharing_a_directory_share_one_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = Settings::for_tests(tmp.path());
        // No tea login: every target is skipped, none may trip over its own lock.
        let targets = [
            (Provider::Forgejo, "x.invalid".to_string()),
            (Provider::Forgejo, "x.invalid".to_string()),
            (Provider::Forgejo, "X.Invalid".to_string()),
        ];
        assert!(sync_hosts(&targets, &settings, &SyncOptions::default()));
    }

    #[test]
    fn incomplete_listing_is_never_pruned() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let base = tmp.path().join("base");
        let host = base.join("h");
        let other = host.join("other").join("repo");
        fs::create_dir_all(&other).unwrap();
        git(&other, &["init", "--quiet"]);
        let settings = Settings::for_tests(&base);
        let h = Listed {
            complete: false,
            ..listed(&host, vec![repo("u/a", &bare)])
        };
        let opts = SyncOptions {
            prune: true,
            force: true,
            ..Default::default()
        };
        sync_listed(&[h], &settings, &opts);
        assert!(other.exists());
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
    fn nested_repos_are_skipped_not_failed() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let base = tmp.path().join("base");
        let host = base.join("h");
        let settings = Settings::for_tests(&base);
        let repos = || vec![repo("g/p", &bare), repo("g/p/sub", &bare)];
        let opts = SyncOptions::default();
        assert_eq!(
            sync_one(&host, repos(), &settings, &opts),
            summary(1, 0, 1, 0)
        );
        assert_eq!(
            sync_one(&host, repos(), &settings, &opts),
            summary(0, 1, 1, 0)
        );
        assert!(!git::is_dirty(&host.join("g").join("p")));
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
    use super::tests::{bare_remote, git};
    use super::*;
    use proptest::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    /// What a local clone holds beyond what the remote has.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Local {
        /// A clean clone: pruning it loses nothing.
        Nothing,
        Untracked,
        Staged,
        UnpushedCommit,
        Stash,
        /// A `.git` git cannot read, so its state is unknown.
        Unreadable,
        /// Committed here and pushed nowhere.
        NoRemote,
    }

    impl Local {
        /// Work that exists in this clone and nowhere else.
        fn is_work(self) -> bool {
            self != Local::Nothing
        }
    }

    /// Build a clone in the requested state under `path`.
    fn plant(path: &Path, state: Local, bare: &Path) {
        if state == Local::Unreadable {
            fs::create_dir_all(path.join(".git")).unwrap();
            return;
        }
        if state == Local::NoRemote {
            fs::create_dir_all(path).unwrap();
            git(path, &["init", "--quiet"]);
            git(path, &["commit", "--quiet", "--allow-empty", "-m", "mine"]);
            return;
        }
        assert_eq!(sync_repo(path, bare.to_str().unwrap()).0, Outcome::Cloned);
        match state {
            Local::Nothing => {}
            Local::Untracked => fs::write(path.join("wip.txt"), "x").unwrap(),
            Local::Staged => {
                fs::write(path.join("wip.txt"), "x").unwrap();
                git(path, &["add", "wip.txt"]);
            }
            Local::UnpushedCommit => {
                git(path, &["commit", "--quiet", "--allow-empty", "-m", "local"]);
            }
            Local::Stash => {
                fs::write(path.join("wip.txt"), "x").unwrap();
                git(path, &["stash", "--quiet", "--include-untracked"]);
            }
            Local::Unreadable | Local::NoRemote => unreachable!("handled above"),
        }
    }

    proptest! {
        // Each case builds real repositories, so keep the count low.
        #![proptest_config(ProptestConfig::with_cases(10))]

        /// The invariant that matters most: a prune deletes a directory only
        /// when it is gone from the remote, inside the filter's scope and
        /// holds nothing that exists nowhere else. The assertion runs both
        /// ways, so nothing is kept without a reason either.
        #[test]
        fn prune_deletes_exactly_the_stale_clones(
            plan in prop::collection::vec(
                (
                    prop::sample::select(vec![
                        Local::Nothing,
                        Local::Untracked,
                        Local::Staged,
                        Local::UnpushedCommit,
                        Local::Stash,
                        Local::Unreadable,
                        Local::NoRemote,
                    ]),
                    any::<bool>(),
                    prop::sample::select(vec!["u", "team"]),
                ),
                1..5,
            ),
            ignore in prop::sample::select(vec!["", "*/r0", "team/*"]),
            filter in prop::sample::select(vec![None, Some("u/*")]),
        ) {
            let tmp = tempfile::tempdir().unwrap();
            let bare = bare_remote(tmp.path());
            let base = tmp.path().join("base");
            let host = base.join("h");
            fs::create_dir_all(&base).unwrap();
            fs::write(base.join(".repos-ignore"), ignore).unwrap();

            let repos: Vec<(String, PathBuf, Local, bool)> = plan
                .iter()
                .enumerate()
                .map(|(i, (state, listed, group))| {
                    let path = host.join(group).join(format!("r{i}"));
                    plant(&path, *state, &bare);
                    (format!("{group}/r{i}"), path, *state, *listed)
                })
                .collect();

            let on_remote: HashSet<PathBuf> = repos
                .iter()
                .filter(|(_, _, _, listed)| *listed)
                .map(|(_, path, _, _)| path.clone())
                .collect();
            let filter = RepoFilter::load(&base, filter.map(str::to_string));
            // --force: the brake on a listing that shrank by more than half is
            // a separate concern with its own test.
            let opts = SyncOptions { prune: true, force: true, ..Default::default() };
            prune(&base, &host, &on_remote, &filter, &opts).unwrap();

            for (name, path, state, listed) in &repos {
                let in_scope = matches!(filter.decide(name), Decision::Keep);
                let stale = !listed && in_scope && !state.is_work();
                // prop_assert_eq! runs the message through concat!, which
                // rules out implicit captures.
                prop_assert_eq!(
                    path.exists(),
                    !stale,
                    "{} {:?} listed={} in_scope={}",
                    name,
                    state,
                    listed,
                    in_scope
                );
            }
        }
    }

    proptest! {
        // Each case clones real repos, so keep the count low.
        #![proptest_config(ProptestConfig::with_cases(6))]

        #[test]
        fn sync_accounts_for_every_repo_and_is_idempotent(
            picked in prop::sample::subsequence(vec!["u/a", "u/b", "u/c", "o/d", "o/bad"], 0..=5),
            ignore in prop::sample::select(vec!["", "u/a", "o/*"]),
        ) {
            let tmp = tempfile::tempdir().unwrap();
            let bare = bare_remote(tmp.path());
            let base = tmp.path().join("base");
            fs::create_dir_all(&base).unwrap();
            fs::write(base.join(".repos-ignore"), ignore).unwrap();
            let host = base.join("h");
            let settings = Settings::for_tests(&base);
            let missing = tmp.path().join("missing.git");
            let repos = || -> Vec<Repo> {
                picked.iter().map(|n| {
                    let url = if n.ends_with("bad") { &missing } else { &bare };
                    let url = url.to_str().unwrap().to_string();
                    Repo { full_name: n.to_string(), ssh_url: url.clone(), https_url: url, ..Default::default() }
                }).collect()
            };
            let run = || {
                let h = Listed { dir: host.clone(), repos: repos(), label: None, complete: true, announced: false };
                sync_listed(&[h], &settings, &SyncOptions::default()).remove(0).unwrap()
            };
            let total = |s: &Summary| s.cloned + s.updated + s.skipped + s.errored;

            let first = run();
            prop_assert_eq!(total(&first), picked.len());
            let second = run();
            prop_assert_eq!(total(&second), picked.len());
            prop_assert_eq!(second.cloned, 0);
            prop_assert_eq!(second.updated, first.cloned + first.updated);
            prop_assert_eq!(second.errored, first.errored);
            for r in find_repos(&host) {
                prop_assert!(!git::is_dirty(&r), "{} left dirty", r.display());
            }
            let _ = git;
        }
    }
}
