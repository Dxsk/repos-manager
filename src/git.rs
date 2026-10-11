use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{Result, bail};

use crate::process;

/// Dead SSH links are dropped after 60s, and prompts (unknown host key,
/// passphrase without agent) fail at once: a background job has no one to
/// answer them and would wait forever.
const SSH_COMMAND: &str = "ssh -o BatchMode=yes -o ServerAliveInterval=15 -o ServerAliveCountMax=4";

/// Run `git -C <dir> <args>` and return trimmed stdout, or stderr as the error.
pub fn run(dir: &Path, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir);
    if matches!(args.first(), Some(&("clone" | "fetch" | "pull" | "remote"))) {
        let mut user = global_settings().clone();
        user.extend(repo_settings(&dir.join(".git").join("config")));
        let env_ssh = ["GIT_SSH_COMMAND", "GIT_SSH"]
            .iter()
            .any(|k| std::env::var_os(k).is_some());
        let (config, ssh) = network_guards(&user, env_ssh);
        for c in config {
            cmd.arg("-c").arg(c);
        }
        if let Some(ssh) = ssh {
            cmd.env("GIT_SSH_COMMAND", ssh);
        }
    }
    cmd.args(args)
        // Never block on a credential prompt from a background job.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never");
    let out = process::output(&mut cmd, process::timeout("GIT", 3600))?;
    if !out.status.success() {
        bail!("{}", one_line(&String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// git's stderr as a single line. Two reasons: a result line is one line per
/// repository, and git's closing advice about access rights sends the reader
/// after a permission problem even when the server said what was really wrong.
fn one_line(stderr: &str) -> String {
    const ADVICE: [&str; 2] = [
        "Please make sure you have the correct access rights",
        "and the repository exists.",
    ];
    let kept: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !ADVICE.contains(l))
        .collect();
    kept.join("; ")
}

const GUARDED: [&str; 3] = ["core.sshcommand", "http.lowspeedlimit", "http.lowspeedtime"];

/// Guarded settings set in the system or global config, read once per run.
fn global_settings() -> &'static Vec<String> {
    static GLOBAL: OnceLock<Vec<String>> = OnceLock::new();
    GLOBAL.get_or_init(|| {
        // Run outside any repository so only system and global levels count.
        let out = Command::new("git")
            .arg("-C")
            .arg(std::env::temp_dir())
            .args(["config", "--get-regexp", r"^(core|http)\."])
            .output();
        let Ok(out) = out else { return Vec::new() };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .map(str::to_lowercase)
            .filter(|k| GUARDED.contains(&k.as_str()))
            .collect()
    })
}

/// Guarded settings in a repo's own config, parsed directly: spawning git
/// for this on every fetch cost about 10% of a sync.
fn repo_settings(config: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(config) else {
        return Vec::new();
    };
    let mut section = String::new();
    let mut found = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(head) = line.strip_prefix('[') {
            section = head
                .split(|c: char| c == ']' || c.is_whitespace() || c == '"')
                .next()
                .unwrap_or_default()
                .to_lowercase();
        } else if let Some(key) = line.split('=').next() {
            let key = format!("{section}.{}", key.trim().to_lowercase());
            if GUARDED.contains(&key.as_str()) {
                found.push(key);
            }
        }
    }
    found
}

/// Stall guards for network commands, skipping anything the user configured.
fn network_guards(user: &[String], env_ssh: bool) -> (Vec<String>, Option<String>) {
    let set = |k: &str| user.iter().any(|u| u == k);
    let mut config = Vec::new();
    // Below 1 KB/s for a minute, an HTTPS transfer is treated as dead.
    if !set("http.lowspeedlimit") && !set("http.lowspeedtime") {
        config.push("http.lowSpeedLimit=1000".to_string());
        config.push("http.lowSpeedTime=60".to_string());
    }
    let ssh = (!env_ssh && !set("core.sshcommand")).then(|| SSH_COMMAND.to_string());
    (config, ssh)
}

/// What every part of the tool calls a clone: a directory holding a `.git`
/// directory. Shared so `status`, the job weighting and the sync recon can
/// never count differently.
pub fn is_clone(dir: &Path) -> bool {
    dir.join(".git").is_dir()
}

pub fn is_dirty(dir: &Path) -> bool {
    run(dir, &["status", "--porcelain"]).is_ok_and(|s| !s.is_empty())
}

pub fn available() -> bool {
    which::which("git").is_ok()
}

pub fn invalid_on_windows(path: &str) -> bool {
    path.split('/').any(|c| {
        let stem = c
            .split('.')
            .next()
            .unwrap_or(c)
            .trim_end()
            .to_ascii_uppercase();
        let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'));
        device
            || c.ends_with(['.', ' '])
            || c.chars().any(|ch| ch < ' ' || r#"<>:"|?*\"#.contains(ch))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim stderr of a clone that a Forgejo instance answered with a 500.
    #[test]
    fn a_git_failure_reads_on_one_line_without_the_advice() {
        let stderr = "Forgejo: Internal Server Error (no message for end users)
fatal: Could not read from remote repository.

Please make sure you have the correct access rights
and the repository exists.
";
        let expected = concat!(
            "Forgejo: Internal Server Error (no message for end users); ",
            "fatal: Could not read from remote repository."
        );
        assert_eq!(one_line(stderr), expected);
    }

    #[test]
    fn a_single_line_failure_is_left_alone() {
        assert_eq!(
            one_line(
                "fatal: couldn't find remote ref HEAD
"
            ),
            "fatal: couldn't find remote ref HEAD"
        );
        assert_eq!(one_line(""), "");
    }

    #[test]
    fn network_guards_never_override_the_user() {
        let (config, ssh) = network_guards(&[], false);
        assert_eq!(config, ["http.lowSpeedLimit=1000", "http.lowSpeedTime=60"]);
        assert_eq!(ssh.as_deref(), Some(SSH_COMMAND));

        let user = [
            "core.sshcommand".to_string(),
            "http.lowspeedtime".to_string(),
        ];
        assert_eq!(network_guards(&user, false), (vec![], None));
        assert_eq!(network_guards(&[], true).1, None);

        // One guard configured leaves the others in place.
        let (config, ssh) = network_guards(&["core.sshcommand".to_string()], false);
        assert_eq!(config, ["http.lowSpeedLimit=1000", "http.lowSpeedTime=60"]);
        assert_eq!(ssh, None);
        let (config, ssh) = network_guards(&["http.lowspeedlimit".to_string()], false);
        assert!(config.is_empty());
        assert_eq!(ssh.as_deref(), Some(SSH_COMMAND));
    }

    #[test]
    fn repo_settings_match_what_git_reads() {
        let tmp = tempfile::tempdir().unwrap();
        run(tmp.path(), &["init", "--quiet"]).unwrap();
        let config = tmp.path().join(".git").join("config");
        assert!(repo_settings(&config).is_empty());
        run(tmp.path(), &["config", "core.sshCommand", "ssh -i key"]).unwrap();
        run(
            tmp.path(),
            &["config", "http.https://h.example.lowSpeedTime", "5"],
        )
        .unwrap();
        run(tmp.path(), &["config", "user.name", "x"]).unwrap();
        assert_eq!(
            repo_settings(&config),
            ["core.sshcommand", "http.lowspeedtime"]
        );
    }

    #[test]
    fn detects_paths_windows_cannot_hold() {
        for bad in [
            r#"web/"Geneva": Renew.md"#,
            "a|b",
            "dir./file",
            "trailing ",
            "CON",
            "x/nul.txt",
            "com1.log",
            "LPT9",
            "back\\slash",
            "tab\there",
        ] {
            assert!(invalid_on_windows(bad), "{bad}");
        }
        for ok in [
            "README.md",
            ".github/ci.yml",
            "console.txt",
            "COM10",
            "a b/c.d",
        ] {
            assert!(!invalid_on_windows(ok), "{ok}");
        }
    }
}
