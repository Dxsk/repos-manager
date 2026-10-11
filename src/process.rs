use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

/// Seconds from `REPOS_MANAGER_<name>_TIMEOUT`, else `default`.
pub fn timeout(name: &str, default: u64) -> Duration {
    let secs = std::env::var(format!("REPOS_MANAGER_{name}_TIMEOUT"))
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&s| s > 0)
        .unwrap_or(default);
    Duration::from_secs(secs)
}

/// Like `Command::output`, but the child is killed once `limit` elapses so a
/// stuck job can never stall a whole sync.
pub fn output(cmd: &mut Command, limit: Duration) -> Result<Output> {
    run(cmd, None, limit)
}

/// Same, with `input` fed to the child's stdin. Some CLIs read their payload
/// from stdin only (`hut graphql`), and they still must not outlive `limit`.
pub fn output_with_input(cmd: &mut Command, input: &[u8], limit: Duration) -> Result<Output> {
    run(cmd, Some(input.to_vec()), limit)
}

fn run(cmd: &mut Command, input: Option<Vec<u8>>, limit: Duration) -> Result<Output> {
    let stdin = match input {
        Some(_) => Stdio::piped(),
        None => Stdio::null(),
    };
    let mut child = cmd
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot run {:?}", cmd.get_program()))?;
    // Written from a thread, and the pipe dropped to signal EOF: a payload
    // bigger than the pipe buffer would otherwise block before we drain.
    if let (Some(input), Some(mut pipe)) = (input, child.stdin.take()) {
        thread::spawn(move || {
            let _ = pipe.write_all(&input);
        });
    }
    // Drain both pipes while waiting, or a chatty child blocks on a full pipe.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let stdout = drain(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = drain(child.stderr.take().map(|p| Box::new(p) as _));

    let start = Instant::now();
    let mut pause = Duration::from_millis(1);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            // The readers are not joined: a grandchild such as ssh may still
            // hold the pipes open.
            bail!(
                "{:?} timed out after {}s",
                cmd.get_program(),
                limit.as_secs()
            );
        }
        thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(50));
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_output_of_a_quick_command() {
        let out = output(
            Command::new("git").arg("--version"),
            Duration::from_secs(30),
        )
        .unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("git version"));
    }

    #[test]
    fn kills_a_command_that_outlives_its_limit() {
        let start = Instant::now();
        let mut cmd = Command::new("git");
        cmd.args(["-c", "alias.wait=!sleep 30", "wait"]);
        let err = output(&mut cmd, Duration::from_millis(500)).unwrap_err();
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn stdin_is_fed_and_closed() {
        let mut cmd = Command::new("git");
        cmd.args(["hash-object", "--stdin"]);
        let out = output_with_input(&mut cmd, b"hello", Duration::from_secs(30)).unwrap();
        assert!(out.status.success(), "{:?}", out);
        // git only answers once stdin reaches EOF.
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0"
        );
    }

    #[test]
    fn a_command_reading_stdin_still_times_out() {
        let mut cmd = Command::new("git");
        cmd.args(["-c", "alias.wait=!sleep 30", "wait"]);
        let err = output_with_input(&mut cmd, b"x", Duration::from_millis(500)).unwrap_err();
        assert!(err.to_string().contains("timed out"), "{err}");
    }

    #[test]
    fn a_large_output_does_not_deadlock() {
        let mut cmd = Command::new("git");
        cmd.args(["-c", "alias.big=!yes | head -c 2000000", "big"]);
        let out = output(&mut cmd, Duration::from_secs(30)).unwrap();
        assert_eq!(out.stdout.len(), 2_000_000);
    }
}
