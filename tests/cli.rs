#[allow(dead_code)]
#[path = "../src/test_server.rs"]
mod test_server;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use test_server::{Response, Server};

fn cmd(args: &[&str]) -> Command {
    let tmp = std::env::temp_dir();
    let mut c = Command::new(env!("CARGO_BIN_EXE_repos-manager"));
    c.args(args)
        .env("NO_COLOR", "1")
        .env("REPOS_MANAGER_NO_UPDATE_CHECK", "1")
        .env(
            "REPOS_MANAGER_CONFIG",
            tmp.join("repos-manager-test-missing.json"),
        );
    c
}

fn run(args: &[&str]) -> Output {
    cmd(args).output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?} failed");
}

#[test]
fn version_flag_and_subcommand() {
    let expected = format!("repos-manager {}", env!("CARGO_PKG_VERSION"));
    for args in [&["--version"][..], &["version"][..]] {
        let o = run(args);
        assert!(o.status.success());
        assert!(stdout(&o).contains(&expected), "{args:?}");
    }
}

#[test]
fn help_lists_providers_and_commands() {
    let o = run(&["--help"]);
    assert!(o.status.success());
    let out = stdout(&o);
    assert!(out.contains("Multi-provider Git repository manager"));
    for word in [
        "github",
        "gitlab",
        "forgejo",
        "bitbucket",
        "radicle",
        "sourcehut",
        "onedev",
        "login",
        "sync",
        "status",
        "init",
        "update",
        "completions",
    ] {
        assert!(out.contains(word), "missing {word}");
    }
}

#[test]
fn no_args_shows_usage() {
    let o = run(&[]);
    assert!(o.status.success());
    assert!(stdout(&o).contains("Usage:"));
}

#[test]
fn unknown_command_fails() {
    assert!(!run(&["nonexistent"]).status.success());
}

#[test]
fn sync_without_all_fails() {
    assert!(!run(&["sync"]).status.success());
}

#[test]
fn provider_without_subcommand_shows_help() {
    let o = run(&["github"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("Usage: repos-manager github"));
}

#[test]
fn completions_are_generated() {
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let o = run(&["completions", shell]);
        assert!(o.status.success(), "{shell}");
        assert!(stdout(&o).contains("repos-manager"), "{shell}");
    }
}

#[test]
fn relative_base_dir_is_rejected() {
    let o = run(&["status", "--base-dir", "relative/path"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("absolute"));
}

#[test]
fn elvish_completions() {
    let o = run(&["completions", "elvish"]);
    assert!(o.status.success());
    assert!(stdout(&o).contains("repos-manager"));
}

#[test]
fn bad_arguments_are_rejected() {
    for args in [
        &["login", "sourceforge"][..],
        &["gitea", "sync", "--parallel", "0"][..],
        &["github", "sync", "--base-dir", "relative"][..],
    ] {
        assert!(!run(args).status.success(), "{args:?}");
    }
}

#[test]
fn sync_all_rejects_host() {
    let o = run(&["sync", "--all", "--host", "x.org"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("<provider> sync --host"));
}

#[test]
fn init_creates_config_once() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("cfg").join("config.json");
    let init = || {
        cmd(&["init"])
            .env("REPOS_MANAGER_CONFIG", &path)
            .output()
            .unwrap()
    };

    let o = init();
    assert!(o.status.success());
    assert!(stdout(&o).contains("Config created"));
    assert!(fs::read_to_string(&path).unwrap().contains("\"hosts\""));

    let o = init();
    assert!(o.status.success());
    assert!(stdout(&o).contains("Config already exists"));
}

#[test]
fn invalid_config_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.json");
    fs::write(&path, "{oops").unwrap();
    let o = cmd(&["status"])
        .env("REPOS_MANAGER_CONFIG", &path)
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(stderr(&o).contains("invalid config file"));
}

#[test]
fn status_reports_dirty_repos() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    let repo = base.join("host").join("me").join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "-b", "main"]);
    git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
    fs::write(repo.join("new.txt"), "x").unwrap();
    let base_arg = base.to_str().unwrap();

    let o = run(&["status", "--base-dir", base_arg, "-v"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("dirty"), "{out}");
    assert!(out.contains("Total: 1 repos - 0 clean, 1 dirty"), "{out}");

    let o = run(&["status", "--base-dir", base_arg, "-q"]);
    assert!(o.status.success());
    assert!(!stdout(&o).contains("Total"));
}

#[test]
fn banner_shows_cached_newer_version() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().join("latest-version");
    fs::write(&cache, "99.0.0").unwrap();
    let o = cmd(&["status", "--base-dir", tmp.path().to_str().unwrap()])
        .env_remove("REPOS_MANAGER_NO_UPDATE_CHECK")
        .env("REPOS_MANAGER_UPDATE_CACHE", &cache)
        // A fresh cache means no background refresh is spawned.
        .env("REPOS_MANAGER_UPDATE_TTL", "86400")
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(stderr(&o).contains("99.0.0 available"), "{}", stderr(&o));
}

fn release_server(tag: &'static str) -> Server {
    Server::start(move |_| Response::json(&format!(r#"{{"tag_name":"{tag}","assets":[]}}"#)))
}

#[test]
fn refresh_update_cache_writes_latest_version() {
    let server = release_server("v9.9.9");
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().join("nested").join("latest-version");
    let o = cmd(&["__refresh-update-cache"])
        .env("REPOS_MANAGER_UPDATE_URL", &server.url)
        .env("REPOS_MANAGER_UPDATE_CACHE", &cache)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(fs::read_to_string(&cache).unwrap(), "9.9.9");
}

#[test]
fn update_when_current_or_declined() {
    let current = release_server("v0.0.1");
    let o = cmd(&["update"])
        .env("REPOS_MANAGER_UPDATE_URL", &current.url)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("Already up to date"));

    // stdin is closed, so the confirmation prompt reads "no" and nothing is replaced.
    let newer = release_server("v99.0.0");
    let o = cmd(&["update"])
        .env("REPOS_MANAGER_UPDATE_URL", &newer.url)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("New version available: 99.0.0"), "{out}");
    assert!(out.contains("Update now?"), "{out}");
}

#[test]
fn update_reports_unreachable_api() {
    let server = Server::start(|_| Response::not_found());
    let o = cmd(&["update"])
        .env("REPOS_MANAGER_UPDATE_URL", &server.url)
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(stderr(&o).contains("cannot reach the release API"));
}

/// A Forgejo instance served locally: tea's config points at the test server,
/// whose repos clone from a local bare remote, so the whole sync runs offline.
struct Forge {
    tmp: tempfile::TempDir,
    server: Server,
    host: String,
    tea_config: std::path::PathBuf,
}

fn forge(repos_status: u16) -> Forge {
    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("remote.git");
    let seed = tmp.path().join("seed");
    fs::create_dir_all(&seed).unwrap();
    git(
        tmp.path(),
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

    let repos = serde_json::json!([{
        "full_name": "me/repo",
        "ssh_url": bare.to_str().unwrap(),
        "html_url": "https://unused/me/repo",
    }])
    .to_string();
    let server = Server::start(move |target| {
        if target.starts_with("/api/v1/user/repos") {
            Response {
                status: repos_status,
                body: repos.clone().into_bytes(),
            }
        } else {
            Response::json("[]")
        }
    });
    let host = server.url.trim_start_matches("http://").to_string();
    let tea_config = tmp.path().join("tea.yml");
    fs::write(
        &tea_config,
        format!(
            "logins:\n  - name: test\n    url: {}\n    token: tok\n",
            server.url
        ),
    )
    .unwrap();
    Forge {
        tmp,
        server,
        host,
        tea_config,
    }
}

impl Forge {
    fn sync(&self, extra: &[&str]) -> Output {
        let base = self.tmp.path().join("base");
        let mut args = vec![
            "forgejo",
            "sync",
            "--host",
            &self.host,
            "--base-dir",
            base.to_str().unwrap(),
        ];
        args.extend_from_slice(extra);
        cmd(&args)
            .env("TEA_CONFIG", &self.tea_config)
            .output()
            .unwrap()
    }
}

#[test]
fn forgejo_sync_clones_then_updates() {
    let f = forge(200);
    let o = f.sync(&["--parallel", "2"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("Found 1 repositories"), "{out}");
    assert!(out.contains("me/repo (cloned)"), "{out}");
    assert!(
        out.contains("Done: 1 cloned, 0 updated, 0 skipped, 0 errors"),
        "{out}"
    );

    let o = f.sync(&["--dry-run", "--prune"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("[dry-run] would update me/repo"));

    let o = f.sync(&["-q"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!stdout(&o).contains("Done:"));
    assert!(f.server.requests().iter().all(|r| r.contains("token tok")));
}

#[test]
fn forgejo_sync_api_failure_fails_the_run() {
    let f = forge(500);
    let o = f.sync(&[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("sync finished with errors"));
}

#[test]
fn unconfigured_hosts_are_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config.json");
    fs::write(
        &config,
        r#"{"hosts":{"forgejo":["a.invalid","b.invalid"]}}"#,
    )
    .unwrap();
    let o = cmd(&[
        "forgejo",
        "sync",
        "--base-dir",
        tmp.path().to_str().unwrap(),
    ])
    .env("REPOS_MANAGER_CONFIG", &config)
    .env("TEA_CONFIG", tmp.path().join("no-tea.yml"))
    .output()
    .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("--- forgejo @ a.invalid ---"), "{out}");
    assert!(
        out.contains("Skipping b.invalid: tea config not found"),
        "{out}"
    );
}

#[test]
fn several_hosts_sync_in_one_run_without_live_progress_in_pipes() {
    let (a, b) = (forge(200), forge(200));
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config.json");
    fs::write(
        &config,
        serde_json::json!({ "hosts": { "forgejo": [a.host, b.host] } }).to_string(),
    )
    .unwrap();
    let tea = tmp.path().join("tea.yml");
    fs::write(
        &tea,
        format!(
            "logins:\n  - name: a\n    url: {}\n    token: tok\n  - name: b\n    url: {}\n    token: tok\n",
            a.server.url, b.server.url
        ),
    )
    .unwrap();
    let base = tmp.path().join("base");
    let o = cmd(&[
        "forgejo",
        "sync",
        "--parallel",
        "2",
        "--base-dir",
        base.to_str().unwrap(),
    ])
    .env("REPOS_MANAGER_CONFIG", &config)
    .env("TEA_CONFIG", &tea)
    .output()
    .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    for f in [&a, &b] {
        let heading = format!("--- forgejo @ {} ---", f.host);
        assert_eq!(out.matches(&heading).count(), 1, "{out}");
        assert!(
            out.contains(&format!("{}/me/repo (cloned)", f.host)),
            "{out}"
        );
        assert!(
            out.contains(&format!("Done {}: 1 cloned, 0 updated", f.host)),
            "{out}"
        );
    }
    assert!(out.contains("[2/2]"), "{out}");
    assert!(!out.contains('⟳') && !out.contains('\x1b'), "{out:?}");
}

/// A bare repo on disk, reachable as a `file://` URL, is a plain git server
/// like any other: it exercises the `any` provider end to end, offline.
fn local_server() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("notes.git");
    let seed = tmp.path().join("seed");
    fs::create_dir_all(&seed).unwrap();
    git(
        tmp.path(),
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

    let path = bare.to_str().unwrap().replace('\\', "/");
    let url = if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    };
    (tmp, url)
}

#[test]
fn any_sync_clones_then_updates_a_configured_url() {
    let (tmp, url) = local_server();
    let config = tmp.path().join("config.json");
    fs::write(
        &config,
        format!(r#"{{"repos":{{"local.invalid":["{url}"]}}}}"#),
    )
    .unwrap();
    let base = tmp.path().join("base");
    let sync = |provider: &str, extra: &[&str]| {
        let mut args = vec![provider, "sync", "--base-dir", base.to_str().unwrap()];
        args.extend_from_slice(extra);
        cmd(&args)
            .env("REPOS_MANAGER_CONFIG", &config)
            .output()
            .unwrap()
    };

    let o = sync("any", &[]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("Found 1 repositories"), "{out}");
    assert!(out.contains("notes (cloned)"), "{out}");
    assert!(base.join("local.invalid").is_dir(), "{out}");

    // `git` is an alias of `any`, and the second run finds the clone in place.
    let o = sync("git", &[]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("notes (up to date)"), "{out}");
    assert!(
        out.contains("Done: 0 cloned, 1 updated, 0 skipped, 0 errors"),
        "{out}"
    );
}

#[test]
fn any_reports_a_host_with_no_repos_and_an_unusable_url() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config.json");
    // `hosts.any` may name a host the `repos` key says nothing about.
    fs::write(
        &config,
        r#"{"hosts":{"any":["listed.invalid","nowhere.invalid"]},
            "repos":{"listed.invalid":["nonsense"],"other.invalid":["git@h:a/b.git"]}}"#,
    )
    .unwrap();
    let o = cmd(&["any", "sync", "--base-dir", tmp.path().to_str().unwrap()])
        .env("REPOS_MANAGER_CONFIG", &config)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("Skipping nowhere.invalid"), "{out}");
    assert!(out.contains("\"repos\""), "{out}");
    // An unparsable URL warns without failing the host.
    assert!(out.contains("nonsense"), "{out}");
}

#[test]
fn self_hosted_only_providers_need_a_host() {
    let tmp = tempfile::tempdir().unwrap();
    for provider in ["onedev", "any"] {
        // --base-dir keeps the run away from the real ~/Documents.
        let o = run(&[provider, "sync", "--base-dir", tmp.path().to_str().unwrap()]);
        assert!(!o.status.success(), "{provider}");
        assert!(
            stderr(&o).contains("no hosts configured"),
            "{provider}: {}",
            stderr(&o)
        );
    }
}

#[test]
fn onedev_skips_a_host_without_credentials() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config.json");
    fs::write(&config, r#"{"hosts":{"onedev":["onedev.invalid"]}}"#).unwrap();
    let o = cmd(&["onedev", "sync", "--base-dir", tmp.path().to_str().unwrap()])
        .env("REPOS_MANAGER_CONFIG", &config)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        stdout(&o).contains("Skipping onedev.invalid: no credentials"),
        "{}",
        stdout(&o)
    );
}

#[test]
fn every_provider_and_alias_is_reachable() {
    for name in [
        "github",
        "gitlab",
        "forgejo",
        "gitea",
        "gogs",
        "bitbucket",
        "radicle",
        "sourcehut",
        "srht",
        "onedev",
        "any",
        "git",
    ] {
        let o = run(&[name, "--help"]);
        assert!(o.status.success(), "{name}: {}", stderr(&o));
        assert!(stdout(&o).contains("sync"), "{name}");
    }
    // `login <provider>` is left out on purpose: it would start a real auth
    // flow. An unknown name is already rejected in bad_arguments_are_rejected.
}

/// A URL the config holds but the provider could not read leaves the listing
/// short of one entry. Pruning it would delete a clone whose remote is alive.
#[test]
fn a_partial_listing_is_never_pruned() {
    let (tmp, url) = local_server();
    let config = tmp.path().join("config.json");
    fs::write(
        &config,
        format!(r#"{{"repos":{{"local.invalid":["{url}","nonsense"]}}}}"#),
    )
    .unwrap();
    let base = tmp.path().join("base");
    let sync = |extra: &[&str]| {
        let mut args = vec!["any", "sync", "--base-dir", base.to_str().unwrap()];
        args.extend_from_slice(extra);
        cmd(&args)
            .env("REPOS_MANAGER_CONFIG", &config)
            .output()
            .unwrap()
    };

    let o = sync(&[]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("notes (cloned)"), "{}", stdout(&o));

    let o = sync(&["--prune"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains("Pruning skipped: the repository list of this host is incomplete"),
        "{out}"
    );
}
