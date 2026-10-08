use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    let tmp = std::env::temp_dir();
    Command::new(env!("CARGO_BIN_EXE_repos-manager"))
        .args(args)
        .env("NO_COLOR", "1")
        .env("REPOS_MANAGER_NO_UPDATE_CHECK", "1")
        .env(
            "REPOS_MANAGER_CONFIG",
            tmp.join("repos-manager-test-missing.json"),
        )
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
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
    assert!(String::from_utf8_lossy(&o.stderr).contains("absolute"));
}
