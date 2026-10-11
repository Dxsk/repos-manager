#[allow(dead_code)]
#[path = "../src/test_server.rs"]
mod test_server;

use std::fs;
use std::path::{Path, PathBuf};
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
    forge_listing(repos_status, &[("me/repo".to_string(), None)])
}

/// `n` repositories named `me/repo0`..., each reporting `size_kb`, or no size
/// at all like the providers that have no size to report.
fn listing(n: usize, size_kb: Option<u64>) -> Vec<(String, Option<u64>)> {
    (0..n).map(|i| (format!("me/repo{i}"), size_kb)).collect()
}

fn forge_listing(repos_status: u16, listed: &[(String, Option<u64>)]) -> Forge {
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

    let items: Vec<serde_json::Value> = listed
        .iter()
        .map(|(name, size_kb)| {
            let mut item = serde_json::json!({
                "full_name": name,
                "ssh_url": bare.to_str().unwrap(),
                "html_url": format!("https://unused/{name}"),
            });
            if let Some(kb) = size_kb {
                item["size"] = (*kb).into();
            }
            item
        })
        .collect();
    let repos = serde_json::Value::Array(items).to_string();
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
    fn base(&self) -> PathBuf {
        self.tmp.path().join("base")
    }

    fn sync_cmd(&self, extra: &[&str]) -> Command {
        let base = self.base();
        let mut args = vec![
            "forgejo",
            "sync",
            "--host",
            &self.host,
            "--base-dir",
            base.to_str().unwrap(),
        ];
        args.extend_from_slice(extra);
        let mut c = cmd(&args);
        c.env("TEA_CONFIG", &self.tea_config);
        c
    }

    fn sync(&self, extra: &[&str]) -> Output {
        self.sync_cmd(extra).output().unwrap()
    }
}

/// Run with both streams pretending to be terminals and `reply` typed at the
/// prompt. `REPOS_MANAGER_FAKE_TTY` exists for this: `cargo test` has no tty.
fn prompted(mut command: Command, reply: &str) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = command
        .env("REPOS_MANAGER_FAKE_TTY", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(reply.as_bytes()).unwrap();
    drop(stdin);
    child.wait_with_output().unwrap()
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
fn bare_url(dir: &Path, name: &str) -> String {
    let bare = dir.join(format!("{name}.git"));
    let seed = dir.join(format!("seed-{name}"));
    fs::create_dir_all(&seed).unwrap();
    git(
        dir,
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
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

fn local_server() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().unwrap();
    let url = bare_url(tmp.path(), "notes");
    (tmp, url)
}

/// Every clone under `root`, by directory name. The `any` provider builds a
/// repo name from the whole URL path, which a temp directory makes
/// unpredictable, so the leaf name is what a test can assert on.
fn clone_names(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            if entry.file_name() == ".git" {
                out.push(dir.file_name().unwrap().to_string_lossy().into_owned());
            } else {
                stack.push(entry.path());
            }
        }
    }
    out.sort();
    out
}

/// Two plain git hosts, `one.invalid` and `two.invalid`, each serving the same
/// two repositories, `keep` and `dropped`.
struct TwoHosts {
    _tmp: tempfile::TempDir,
    config: PathBuf,
    base: PathBuf,
    keep: String,
}

fn two_hosts(filters: Option<&str>) -> TwoHosts {
    let tmp = tempfile::tempdir().unwrap();
    let keep = bare_url(tmp.path(), "keep");
    let other = bare_url(tmp.path(), "dropped");
    let config = tmp.path().join("config.json");
    fs::write(
        &config,
        format!(
            r#"{{"repos":{{"one.invalid":["{keep}","{other}"],
                           "two.invalid":["{keep}","{other}"]}}}}"#
        ),
    )
    .unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    if let Some(yaml) = filters {
        fs::write(base.join(".repos.yml"), yaml).unwrap();
    }
    TwoHosts {
        _tmp: tmp,
        config,
        base,
        keep,
    }
}

fn any_sync(config: &Path, base: &Path, extra: &[&str]) -> Output {
    let mut args = vec!["any", "sync", "--base-dir", base.to_str().unwrap()];
    args.extend_from_slice(extra);
    cmd(&args)
        .env("REPOS_MANAGER_CONFIG", config)
        .output()
        .unwrap()
}

#[test]
fn a_host_block_restricts_that_host_only() {
    let h = two_hosts(Some("hosts:\n  one.invalid:\n    only: ['*/keep']\n"));
    let o = any_sync(&h.config, &h.base, &[]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("Done one.invalid: 1 cloned"), "{out}");
    assert!(out.contains("Done two.invalid: 2 cloned"), "{out}");
    assert_eq!(clone_names(&h.base.join("one.invalid")), ["keep"]);
    assert_eq!(
        clone_names(&h.base.join("two.invalid")),
        ["dropped", "keep"]
    );
}

/// A repo a host block leaves out is not that sync's business, so a prune
/// driven by the same host must not delete it.
#[test]
fn a_repo_outside_a_host_block_is_never_pruned() {
    let h = two_hosts(None);
    let o = any_sync(&h.config, &h.base, &[]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        clone_names(&h.base.join("one.invalid")),
        ["dropped", "keep"]
    );

    // `dropped` is gone from the remote, and out of this host's scope.
    fs::write(
        &h.config,
        format!(r#"{{"repos":{{"one.invalid":["{}"]}}}}"#, h.keep),
    )
    .unwrap();
    fs::write(
        h.base.join(".repos.yml"),
        "hosts:\n  one.invalid:\n    only: ['*/keep']\n",
    )
    .unwrap();

    let o = any_sync(&h.config, &h.base, &["--prune"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(!out.contains("pruned"), "{out}");
    assert_eq!(
        clone_names(&h.base.join("one.invalid")),
        ["dropped", "keep"]
    );
}

/// Two allow-lists that cannot both match leave nothing to sync, which looks
/// exactly like an empty remote unless both sources are named.
#[test]
fn stacked_allow_lists_with_no_overlap_warn_about_the_empty_scope() {
    let h = two_hosts(Some(
        "only: ['*/keep']\nhosts:\n  one.invalid:\n    only: ['*/dropped']\n",
    ));
    let o = any_sync(&h.config, &h.base, &[]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains("No repository of one.invalid is in scope"),
        "{out}"
    );
    assert!(out.contains("'hosts.one.invalid.only'"), "{out}");
    assert!(out.contains("'only'"), "{out}");
    assert!(out.contains("Done one.invalid: 0 cloned"), "{out}");
    assert!(clone_names(&h.base.join("one.invalid")).is_empty());
    // The global list alone still applies to the other host.
    assert_eq!(clone_names(&h.base.join("two.invalid")), ["keep"]);
}

/// The 1.0 text files are documented, so they keep working on their own.
#[test]
fn legacy_text_files_still_filter_without_a_yaml_file() {
    let h = two_hosts(None);
    fs::write(h.base.join(".repos-filter"), "*/keep\n*/dropped # both\n").unwrap();
    fs::write(h.base.join(".repos-ignore"), "*/dropped\n").unwrap();
    let o = any_sync(&h.config, &h.base, &[]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("(ignored)"), "{out}");
    assert_eq!(clone_names(&h.base.join("one.invalid")), ["keep"]);

    // Once a .repos.yml sits next to them, they are announced as ignored.
    fs::write(h.base.join(".repos.yml"), "ignore: ['*/nothing']\n").unwrap();
    let o = any_sync(&h.config, &h.base, &[]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains(".repos-filter and .repos-ignore are ignored"),
        "{out}"
    );
    assert_eq!(
        clone_names(&h.base.join("one.invalid")),
        ["dropped", "keep"]
    );
}

/// A typo such as `onlyy:` would widen the scope in silence, and `--prune`
/// acts on that scope, so the run has to stop instead.
#[test]
fn an_unknown_key_in_the_filter_file_fails_the_run() {
    let h = two_hosts(Some("onlyy: ['*/keep']\n"));
    let o = any_sync(&h.config, &h.base, &["--prune"]);
    assert!(!o.status.success());
    let out = stdout(&o);
    assert!(out.contains(".repos.yml"), "{out}");
    assert!(out.contains("onlyy"), "{out}");
    assert!(clone_names(&h.base).is_empty(), "{out}");
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

/// 200 MB each, six of them: over the default thresholds.
fn big_forge() -> Forge {
    forge_listing(200, &listing(6, Some(200_000)))
}

/// The whole promise of the recon: a daily sync, where nothing is new and
/// nothing is big, says not one word more than it always has.
#[test]
fn a_small_sync_is_never_summarized_nor_questioned() {
    let f = forge_listing(200, &listing(2, Some(1024)));
    for pass in ["clone", "update"] {
        // A terminal changes nothing: the thresholds are what keep it quiet.
        let o = prompted(f.sync_cmd(&[]), "");
        assert!(o.status.success(), "{pass}: {}", stderr(&o));
        let out = stdout(&o);
        assert!(!out.contains("in scope"), "{pass}: {out}");
        assert!(!out.contains("Proceed?"), "{pass}: {out}");
        assert!(!out.contains("to download"), "{pass}: {out}");
        assert!(out.contains("Found 2 repositories"), "{pass}: {out}");
    }
    let o = f.sync(&[]);
    assert!(
        stdout(&o).contains("Done: 0 cloned, 2 updated, 0 skipped, 0 errors"),
        "{}",
        stdout(&o)
    );
}

/// Cron and CI: no terminal, so no question, but the summary is still printed
/// and the sync carries on.
#[test]
fn without_a_terminal_the_summary_prints_and_the_sync_goes_on() {
    let f = big_forge();
    let o = f.sync(&[]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains("6 in scope, 0 cloned, 6 new (1.1 GB)"),
        "{out}"
    );
    assert!(out.contains("biggest new: me/repo0 195 MB"), "{out}");
    assert!(
        out.contains("6 clones, 0 updates, about 1.1 GB to download."),
        "{out}"
    );
    assert!(!out.contains("Proceed?"), "{out}");
    assert_eq!(clone_names(&f.base()).len(), 6, "{out}");
}

#[test]
fn over_the_threshold_one_question_covers_the_run() {
    let f = big_forge();
    let o = prompted(f.sync_cmd(&[]), "n\n");
    assert!(o.status.success(), "declining is not a failure");
    let out = stdout(&o);
    assert_eq!(out.matches("Proceed? [Y/n]").count(), 1, "{out}");
    assert!(
        out.contains("Nothing was cloned. Narrow the scope"),
        "{out}"
    );
    for hint in ["--filter", "--ignore", ".repos.yml"] {
        assert!(out.contains(hint), "{out}");
    }
    assert!(out.contains("6 in scope"), "{out}");
    assert!(clone_names(&f.base()).is_empty(), "{out}");

    // An empty line is the default, and the default is yes.
    let o = prompted(f.sync_cmd(&[]), "\n");
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(clone_names(&f.base()).len(), 6, "{}", stdout(&o));
}

#[test]
fn dry_run_and_yes_summarize_without_asking() {
    let f = big_forge();
    let o = prompted(f.sync_cmd(&["--dry-run"]), "");
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("6 clones, 0 updates, about 1.1 GB"), "{out}");
    assert!(!out.contains("Proceed?"), "{out}");
    assert!(clone_names(&f.base()).is_empty(), "{out}");

    let o = prompted(f.sync_cmd(&["-y"]), "");
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("6 in scope, 0 cloned, 6 new"), "{out}");
    assert!(!out.contains("Proceed?"), "{out}");
    assert_eq!(clone_names(&f.base()).len(), 6, "{out}");
}

/// Plain git servers report no size at all, and one unreadable URL makes the
/// listing short: the summary has to own up to both before asking.
#[test]
fn an_unknown_volume_and_a_short_listing_are_named_before_the_question() {
    let tmp = tempfile::tempdir().unwrap();
    let urls: Vec<String> = (0..5)
        .map(|i| format!("\"{}\"", bare_url(tmp.path(), &format!("r{i}"))))
        .collect();
    let config = tmp.path().join("config.json");
    fs::write(
        &config,
        format!(
            r#"{{"repos":{{"local.invalid":[{},"nonsense"]}}}}"#,
            urls.join(",")
        ),
    )
    .unwrap();
    let base = tmp.path().join("base");
    let mut command = cmd(&[
        "any",
        "sync",
        "--prune",
        "--base-dir",
        base.to_str().unwrap(),
    ]);
    command.env("REPOS_MANAGER_CONFIG", &config);

    let o = prompted(command, "n\n");
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("5 in scope, 0 cloned, 5 new (?)"), "{out}");
    assert!(
        out.contains("with --prune: skipped, the repository list of this host is incomplete"),
        "{out}"
    );
    assert!(
        out.contains("5 clones, 0 updates, unknown download volume."),
        "{out}"
    );
    assert!(!out.contains("biggest new"), "{out}");
    assert_eq!(out.matches("Proceed? [Y/n]").count(), 1, "{out}");
    assert!(clone_names(&base).is_empty(), "{out}");
}

/// `confirm_sync` lives in config.json: the machine decides when to ask, the
/// workspace's `.repos.yml` decides what is in scope.
#[test]
fn confirm_sync_never_still_prints_the_summary() {
    let f = big_forge();
    let config = f.tmp.path().join("config.json");
    fs::write(&config, r#"{"confirm_sync":"never"}"#).unwrap();
    let mut command = f.sync_cmd(&[]);
    command.env("REPOS_MANAGER_CONFIG", &config);
    let o = prompted(command, "n\n");
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("6 in scope, 0 cloned, 6 new"), "{out}");
    assert!(!out.contains("Proceed?"), "{out}");
    assert_eq!(clone_names(&f.base()).len(), 6, "{out}");
}

/// `confirm_sync: always` asks even for a single update, and a lower
/// `confirm_new_repos` moves the line where the question appears.
#[test]
fn confirm_sync_always_asks_for_anything_to_do() {
    let f = forge_listing(200, &listing(1, Some(1024)));
    let config = f.tmp.path().join("config.json");
    fs::write(&config, r#"{"confirm_sync":"always"}"#).unwrap();
    let ask = |reply: &str| {
        let mut command = f.sync_cmd(&[]);
        command.env("REPOS_MANAGER_CONFIG", &config);
        prompted(command, reply)
    };
    let o = ask("n\n");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        stdout(&o).contains("1 in scope, 0 cloned, 1 new"),
        "{}",
        stdout(&o)
    );
    assert!(clone_names(&f.base()).is_empty());

    let o = ask("y\n");
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(clone_names(&f.base()), ["repo0"]);
}
