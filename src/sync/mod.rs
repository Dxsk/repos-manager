//! Per-host orchestration of a sync: listing, scope, recon, then the jobs.

mod jobs;
mod paths;
mod prune;

use std::collections::HashSet;
use std::fs::{self, File, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;

use anyhow::{Result, bail};

use crate::config::Settings;
use crate::git;
use crate::matcher::{Decision, HostFilter, RepoFilter};
use crate::output;
use crate::plan::{self, Plan};
use crate::providers::{ListError, Provider, Repo};
use jobs::{Job, run_parallel};
use paths::{Fs, host_dir, origin_url, path_conflicts, repo_name, repo_path};
use prune::{prunable, prune};

#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    pub only: Vec<String>,
    pub ignore: Vec<String>,
    pub prune: bool,
    pub force: bool,
    pub dry_run: bool,
    /// `-y`: print the recon summary but never ask.
    pub assume_yes: bool,
    /// `--plan`: print the recon summary and stop before the first clone.
    pub plan: bool,
    /// `--write-ignore`: write `<base_dir>/.repos.yml` and stop.
    pub write_ignore: bool,
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
                bail!("another sync is already running on {}", output::path(dir))
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

struct Listed {
    dir: PathBuf,
    repos: Vec<Repo>,
    /// Selects the filter rules. Providers may share it, the first one names the directory.
    host: String,
    heading: Option<String>,
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

/// The host directory, not the (provider, host) pair, is what the lock and
/// prune act on, so targets sharing one are listed together under one lock.
/// Hostnames are case-insensitive, and so is the Windows filesystem.
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

/// Providers sharing a host directory list into it together: a project both
/// report is one repository, and the first listing wins.
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

    // A run that only reports takes no lock and does not create the host
    // directory just to say what it would hold.
    let read_only = opts.plan || opts.write_ignore || opts.dry_run;

    let listings: Vec<_> = thread::scope(|s| {
        let handles: Vec<_> = groups
            .iter()
            .map(|Group { dir, members, .. }| {
                s.spawn(move || -> Result<_> {
                    let lock = (!read_only).then(|| HostLock::acquire(dir)).transpose()?;
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
                locks.extend(lock);
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
                Ok(listing) => {
                    any = true;
                    // A listing missing entries must never drive a prune.
                    complete &= !listing.partial;
                    merge_unique(&mut repos, listing.repos);
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
                host: host.to_string(),
                heading: multi.then(|| heading.clone()),
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

/// Every listed path (which prune must never touch), the repos in scope and the dropped count.
type Scope<'a> = (HashSet<PathBuf>, Vec<(&'a Repo, PathBuf)>, usize);

fn in_scope<'a>(h: &'a Listed, filter: &HostFilter<'_>) -> Scope<'a> {
    let mut listed = HashSet::new();
    let mut skipped = 0;
    let mut kept = Vec::new();
    for repo in &h.repos {
        let full_name = &repo.full_name;
        let path = repo_path(&h.dir, full_name);
        listed.insert(path.clone());
        match filter.decide(full_name) {
            Decision::Drop => {}
            Decision::Ignored => {
                output::skip(&format!("{full_name} (ignored)"));
                skipped += 1;
            }
            Decision::Keep => kept.push((repo, path)),
        }
    }
    if kept.is_empty() && !h.repos.is_empty() {
        warn_empty_scope(&h.host, filter);
    }
    (listed, kept, skipped)
}

/// Stacked allow-lists can leave a host with nothing to sync, which on screen
/// looks exactly like an empty remote. Name every layer that narrowed it.
fn warn_empty_scope(host: &str, filter: &HostFilter<'_>) {
    let sources = filter.only_sources();
    if sources.len() < 2 {
        return;
    }
    let named: Vec<String> = sources.iter().map(|s| format!("'{s}'")).collect();
    output::warn(&format!(
        "No repository of {host} is in scope: {} have nothing in common",
        named.join(" and ")
    ));
}

fn collect_host(
    index: usize,
    h: &Listed,
    filter: &HostFilter<'_>,
    settings: &Settings,
    opts: &SyncOptions,
    jobs: &mut Vec<Job>,
) -> (HashSet<PathBuf>, usize, plan::HostRow) {
    let (listed, kept, mut skipped) = in_scope(h, filter);
    let url = |r: &Repo| r.clone_url(settings.use_https).to_string();
    let candidates: Vec<_> = kept
        .iter()
        .map(|(r, p)| (repo_name(&h.dir, p), p.as_path(), url(r)))
        .collect();
    let conflicts = path_conflicts(&candidates, Fs::current(), origin_url);
    let mut row = plan::HostRow {
        host: h.host.clone(),
        incomplete: !h.complete,
        ..Default::default()
    };
    for (n, (repo, path)) in kept.into_iter().enumerate() {
        let full_name = &repo.full_name;
        if let Some(reason) = conflicts.get(&n) {
            output::warn(&format!("{full_name} (skipped: {reason})"));
            skipped += 1;
            continue;
        }
        // One answer for the recon, the dry run and the job weight alike.
        let on_disk = git::is_clone(&path);
        row.repos.push(plan::ScopeRepo {
            name: full_name.clone(),
            size_kb: repo.size_kb,
            cloned: on_disk,
        });
        if opts.dry_run {
            let verb = if on_disk { "update" } else { "clone" };
            output::info(&format!("  [dry-run] would {verb} {full_name}"));
            continue;
        }
        jobs.push(Job {
            host: index,
            weight: if on_disk {
                0
            } else {
                repo.size_kb.unwrap_or(0) + 1
            },
            path,
            url: url(repo),
            name: if h.heading.is_some() {
                format!("{}/{full_name}", h.host)
            } else {
                full_name.clone()
            },
        });
    }
    // A host whose listing is short already has prune disabled, and the
    // summary says so instead of promising a count.
    row.prunable = (opts.prune && h.complete).then(|| prunable(&h.dir, &listed, filter));
    (listed, skipped, row)
}

/// `--plan` and `--write-ignore` print whatever `-q` says, since printing is
/// the whole point of the flag.
fn recon_only(
    plan: &Plan,
    filter: &RepoFilter,
    settings: &Settings,
    opts: &SyncOptions,
) -> Vec<Result<Summary>> {
    if opts.plan {
        println!("\n{}\n", plan::render(plan));
    }
    if opts.write_ignore {
        let content = plan::ignore_file(plan, filter, &plan::today());
        match plan::write_ignore_file(&settings.base_dir, &content, opts.force) {
            Ok(path) => println!(
                "Wrote {}\nUncomment a repository to stop syncing it, then run the sync again.",
                output::path(&path)
            ),
            Err(e) => return vec![Err(e)],
        }
    }
    Vec::new()
}

/// Runs once the scope is known for every host and before any clone. `Some`
/// means the run goes no further; an empty result that it stopped cleanly.
fn recon(
    plan: &Plan,
    filter: &RepoFilter,
    settings: &Settings,
    opts: &SyncOptions,
) -> Option<Vec<Result<Summary>>> {
    if opts.plan || opts.write_ignore {
        return Some(recon_only(plan, filter, settings, opts));
    }
    let skip_prompt = opts.assume_yes || opts.dry_run;
    let approved = plan::approved(plan, &settings.confirm, skip_prompt, &settings.base_dir);
    (!approved).then(Vec::new)
}

/// An empty result means the user declined: nothing ran, and that is not an error.
fn sync_listed(hosts: &[Listed], settings: &Settings, opts: &SyncOptions) -> Vec<Result<Summary>> {
    // A broken filter file must stop every host: it would otherwise run with
    // a wider scope than the user wrote, and --prune acts on that scope.
    let filter = match RepoFilter::load(&settings.base_dir, &opts.only, &opts.ignore) {
        Ok(f) => f,
        Err(e) => return vec![Err(e)],
    };
    let filters: Vec<HostFilter> = hosts.iter().map(|h| filter.for_host(&h.host)).collect();
    let mut jobs = Vec::new();
    let mut plans: Vec<(HashSet<PathBuf>, usize)> = Vec::new();
    let mut plan = Plan {
        prune: opts.prune,
        ..Default::default()
    };

    for ((i, h), host_filter) in hosts.iter().enumerate().zip(&filters) {
        if let Some(heading) = &h.heading
            && !h.announced
        {
            output::heading(heading);
        }
        output::info(&format!("Found {} repositories", h.repos.len()));
        output::blank();
        let (listed, skipped, row) = collect_host(i, h, host_filter, settings, opts, &mut jobs);
        plan.hosts.push(row);
        plans.push((listed, skipped));
    }

    if let Some(stopped) = recon(&plan, &filter, settings, opts) {
        return stopped;
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
                prune(&settings.base_dir, &h.dir, &listed, &filters[i], opts)?;
            }
            let summary = Summary {
                cloned: count(Outcome::Cloned),
                updated: count(Outcome::Updated),
                skipped: count(Outcome::Skipped) + skipped,
                errored: count(Outcome::Errored),
            };
            let label = if h.heading.is_some() {
                format!(" {}", h.host)
            } else {
                output::blank();
                String::new()
            };
            output::info(&format!(
                "Done{label}: {} cloned, {} updated, {} skipped, {} errors",
                summary.cloned, summary.updated, summary.skipped, summary.errored,
            ));
            Ok(summary)
        })
        .collect()
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

    pub(super) fn init_repo(p: &Path) {
        fs::create_dir_all(p).unwrap();
        git(p, &["init", "--quiet"]);
    }

    pub(super) fn repo(name: &str, url: &Path) -> Repo {
        let url = url.to_str().unwrap().to_string();
        Repo {
            full_name: name.into(),
            ssh_url: url.clone(),
            https_url: url,
            ..Default::default()
        }
    }

    pub(super) fn listed(dir: &Path, repos: Vec<Repo>) -> Listed {
        Listed {
            dir: dir.to_path_buf(),
            repos,
            host: "h".into(),
            heading: None,
            complete: true,
            announced: false,
        }
    }

    pub(super) fn sync_one(
        dir: &Path,
        repos: Vec<Repo>,
        settings: &Settings,
        opts: &SyncOptions,
    ) -> Summary {
        let mut out = sync_listed(&[listed(dir, repos)], settings, opts);
        out.remove(0).unwrap()
    }

    pub(super) fn summary(
        cloned: usize,
        updated: usize,
        skipped: usize,
        errored: usize,
    ) -> Summary {
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
    fn sync_listed_applies_yaml_filters_per_host() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let base = tmp.path().join("base");
        fs::create_dir_all(&base).unwrap();
        fs::write(
            base.join(".repos.yml"),
            "only: [u/*]
ignore: [u/ignored]
hosts:
  two:
    ignore: [u/a]
",
        )
        .unwrap();
        let settings = Settings::for_tests(&base);
        let repos = || {
            vec![
                repo("u/a", &bare),
                repo("u/ignored", &bare),
                repo("other/dropped", &bare),
            ]
        };
        let hosts = [
            Listed {
                host: "one".into(),
                heading: Some("one".into()),
                ..listed(&base.join("one"), repos())
            },
            Listed {
                host: "two".into(),
                heading: Some("two".into()),
                ..listed(&base.join("two"), repos())
            },
        ];
        let out: Vec<_> = sync_listed(&hosts, &settings, &SyncOptions::default())
            .into_iter()
            .map(Result::unwrap)
            .collect();
        // other/dropped is out of the global scope on both hosts, and the
        // host block of `two` ignores u/a there and nowhere else.
        assert_eq!(out, [summary(1, 0, 1, 0), summary(0, 0, 2, 0)]);
        assert!(base.join("one").join("u").join("a").join(".git").is_dir());
        assert!(!base.join("two").join("u").join("a").exists());
    }

    #[test]
    fn a_broken_filter_file_stops_every_host() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = bare_remote(tmp.path());
        let base = tmp.path().join("base");
        let host = base.join("h");
        fs::create_dir_all(&base).unwrap();
        fs::write(
            base.join(".repos.yml"),
            "onlyy: [u/*]
",
        )
        .unwrap();
        let settings = Settings::for_tests(&base);
        let hosts = [listed(&host, vec![repo("u/a", &bare)])];
        let out = sync_listed(&hosts, &settings, &SyncOptions::default());
        assert_eq!(out.len(), 1);
        let err = out[0].as_ref().unwrap_err().to_string();
        assert!(err.contains(".repos.yml"), "{err}");
        assert!(!host.exists(), "a job ran on a broken filter file");
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
                host: "a".into(),
                heading: Some("a".into()),
                ..listed(&a, vec![repo("u/x", &bare), repo("u/y", &bare)])
            },
            Listed {
                host: "b".into(),
                heading: Some("b".into()),
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
}

#[cfg(test)]
mod properties {
    use super::paths::find_repos;
    use super::tests::{bare_remote, git};
    use super::*;
    use proptest::prelude::*;

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
                let h = Listed {
                    dir: host.clone(),
                    repos: repos(),
                    host: "h".into(),
                    heading: None,
                    complete: true,
                    announced: false,
                };
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
