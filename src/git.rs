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
