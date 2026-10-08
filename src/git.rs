use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

/// Run `git -C <dir> <args>` and return trimmed stdout, or stderr as the error.
pub fn run(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        // Never block on a credential prompt from a background job.
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn is_dirty(dir: &Path) -> bool {
    run(dir, &["status", "--porcelain"]).is_ok_and(|s| !s.is_empty())
}

pub fn available() -> bool {
    which::which("git").is_ok()
}
