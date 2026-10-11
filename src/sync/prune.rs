//! Deleting repos the remote no longer lists, and the guards around it.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::SyncOptions;
use super::paths::{find_repos, repo_name};
use crate::git;
use crate::matcher::{Decision, HostFilter};
use crate::output;

fn prune_candidates<'a>(
    host_dir: &Path,
    local: &'a [PathBuf],
    listed: &HashSet<PathBuf>,
    filter: &HostFilter<'_>,
) -> Vec<&'a PathBuf> {
    local
        .iter()
        .filter(|r| !listed.contains(*r))
        .filter(|r| matches!(filter.decide(&repo_name(host_dir, r)), Decision::Keep))
        .collect()
}

/// For the summary before the confirmation. Local-only work is not checked
/// (a git call per candidate, and prune refuses it anyway): an upper bound.
pub(super) fn prunable(
    host_dir: &Path,
    listed: &HashSet<PathBuf>,
    filter: &HostFilter<'_>,
) -> usize {
    prune_candidates(host_dir, &find_repos(host_dir), listed, filter).len()
}

/// Deletion is irreversible while the remote listing can be wrong (renamed
/// repo, revoked access, truncated API answer), so a candidate must be
/// unlisted, in the filter's scope and free of local-only work. The filter is
/// the one of the host being pruned, never a wider set of rules.
pub fn prune(
    base: &Path,
    host_dir: &Path,
    listed: &HashSet<PathBuf>,
    filter: &HostFilter<'_>,
    opts: &SyncOptions,
) -> Result<()> {
    if !host_dir.is_dir() {
        return Ok(());
    }
    let (real_host, real_base) = (host_dir.canonicalize()?, base.canonicalize()?);
    if !real_host.starts_with(&real_base) || real_host == real_base {
        bail!(
            "prune aborted: {} is outside the base directory",
            output::path(host_dir)
        );
    }

    let local = find_repos(host_dir);
    let candidates = prune_candidates(host_dir, &local, listed, filter);
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
        let rel = output::path(repo.strip_prefix(base).unwrap_or(repo));
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
    if let Some(msg) = pruned_summary(pruned, opts.dry_run) {
        output::warn(&msg);
    }
    Ok(())
}

/// A dry run deletes nothing, so it must not report having pruned.
fn pruned_summary(count: usize, dry_run: bool) -> Option<String> {
    let verb = if dry_run { "Would prune" } else { "Pruned" };
    let plural = if count == 1 {
        "repository"
    } else {
        "repositories"
    };
    (count > 0).then(|| format!("{verb} {count} {plural}"))
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
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::matcher::RepoFilter;
    use crate::sync::Outcome;
    use crate::sync::jobs::sync_repo;
    use crate::sync::tests::{bare_remote, git, init_repo, repo, sync_one};
    #[test]
    fn prune_leaves_a_repo_outside_the_host_block_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let host = base.join("h");
        fs::create_dir_all(&base).unwrap();
        fs::write(
            base.join(".repos.yml"),
            "hosts:
  h:
    only: [team/*]
",
        )
        .unwrap();
        init_repo(&host.join("team").join("gone"));
        init_repo(&host.join("other").join("kept"));
        let rules = RepoFilter::load(&base, &[], &[]).unwrap();
        let none = HashSet::new();
        prune(
            &base,
            &host,
            &none,
            &rules.for_host("h"),
            &prune_opts(false, true),
        )
        .unwrap();
        assert!(!host.join("team").join("gone").exists());
        assert!(host.join("other").join("kept").exists());
        // The block narrows this host only: elsewhere the same name is in scope.
        assert!(matches!(
            rules.for_host("elsewhere").decide("other/kept"),
            Decision::Keep
        ));
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
        RepoFilter::load(base, &[], &[]).unwrap()
    }

    /// What the summary announces before the question comes from the same
    /// selection prune itself acts on.
    #[test]
    fn prunable_counts_what_prune_would_act_on() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let host = base.join("host");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join(".repos.yml"), "ignore: [u/spared]\n").unwrap();
        for leaf in ["keep", "gone", "spared"] {
            init_repo(&host.join("u").join(leaf));
        }
        let listed: HashSet<PathBuf> = [host.join("u").join("keep")].into();
        let rules = no_filter(&base);
        let filter = rules.for_host("host");
        // u/keep is still listed and u/spared is out of scope: only u/gone.
        assert_eq!(prunable(&host, &listed, &filter), 1);
        assert_eq!(prunable(&host, &HashSet::new(), &filter), 2);
        // A host with nothing on disk yet is not a half-read listing.
        assert_eq!(prunable(&base.join("other"), &listed, &filter), 0);
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
            &no_filter(&base).for_host("h"),
            &prune_opts(true, false),
        )
        .unwrap();
        assert!(gone.exists());
        prune(
            &base,
            &host,
            &listed,
            &no_filter(&base).for_host("h"),
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
        prune(
            &base,
            &host,
            &listed,
            &no_filter(&base).for_host("h"),
            &opts,
        )
        .unwrap();
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
            only: vec!["u/*".into()],
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
        let rules = no_filter(&base);
        let filter = rules.for_host("h");

        let err = prune(&base, &host, &listed, &filter, &prune_opts(false, false)).unwrap_err();
        assert!(err.to_string().contains("2 of 3"), "{err}");
        prune(&base, &host, &listed, &filter, &prune_opts(true, false)).unwrap();
        assert!(repos.iter().all(|r| r.exists()));

        prune(&base, &host, &listed, &filter, &prune_opts(false, true)).unwrap();
        assert!(repos[0].exists() && !repos[1].exists() && !repos[2].exists());
    }

    #[test]
    fn a_dry_run_never_reports_having_pruned() {
        assert_eq!(pruned_summary(0, true), None);
        assert_eq!(pruned_summary(0, false), None);
        assert_eq!(
            pruned_summary(2, true).unwrap(),
            "Would prune 2 repositories"
        );
        assert_eq!(pruned_summary(2, false).unwrap(), "Pruned 2 repositories");
        assert_eq!(pruned_summary(1, false).unwrap(), "Pruned 1 repository");
    }

    #[test]
    fn prune_stops_once_more_than_half_a_host_would_go() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let rules = no_filter(&base);
        let filter = rules.for_host("h");
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
        let (none, rules) = (HashSet::new(), no_filter(&base));
        let filter = rules.for_host("h");
        let opts = prune_opts(false, true);
        prune(&base, &base.join("missing"), &none, &filter, &opts).unwrap();
        assert!(prune(&base, &base, &none, &filter, &opts).is_err());
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        assert!(prune(&base, &outside, &none, &filter, &opts).is_err());
    }
}

#[cfg(test)]
mod properties {
    use super::*;
    use crate::matcher::RepoFilter;
    use crate::sync::Outcome;
    use crate::sync::jobs::sync_repo;
    use crate::sync::tests::{bare_remote, git};
    use proptest::prelude::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Local {
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
            let cli: Vec<String> = filter.into_iter().map(str::to_string).collect();
            let rules = RepoFilter::load(&base, &cli, &[]).unwrap();
            let filter = rules.for_host("h");
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
}
