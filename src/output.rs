use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

static QUIET: AtomicBool = AtomicBool::new(false);
static VERBOSE: AtomicBool = AtomicBool::new(false);
static COLOR: OnceLock<bool> = OnceLock::new();

pub fn set_quiet(v: bool) {
    QUIET.store(v, Ordering::Relaxed);
}

pub fn set_verbose(v: bool) {
    VERBOSE.store(v, Ordering::Relaxed);
}

pub fn is_quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

/// Colors follow https://no-color.org/ and are disabled when stdout is not a TTY.
fn color_enabled() -> bool {
    *COLOR.get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal())
}

fn paint(code: &str, s: &str) -> String {
    if color_enabled() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn red(s: &str) -> String {
    paint("31", s)
}
pub fn green(s: &str) -> String {
    paint("32", s)
}
pub fn yellow(s: &str) -> String {
    paint("33", s)
}
pub fn blue(s: &str) -> String {
    paint("34", s)
}
pub fn gray(s: &str) -> String {
    paint("90", s)
}
pub fn bold(s: &str) -> String {
    paint("1", s)
}

pub fn info(msg: &str) {
    if !is_quiet() {
        println!("{}", blue(msg));
    }
}

pub fn success(msg: &str) {
    if !is_quiet() {
        println!("  {}", green(&format!("✓ {msg}")));
    }
}

pub fn warn(msg: &str) {
    println!("  {}", yellow(&format!("⚠ {msg}")));
}

pub fn error(msg: &str) {
    println!("  {}", red(&format!("✗ {msg}")));
}

pub fn skip(msg: &str) {
    if !is_quiet() {
        println!("  {}", gray(&format!("⊘ {msg}")));
    }
}

pub fn debug(msg: &str) {
    if VERBOSE.load(Ordering::Relaxed) {
        println!("  {}", gray(&format!("[debug] {msg}")));
    }
}

pub fn heading(msg: &str) {
    println!("\n{}\n", bold(msg));
}

pub fn blank() {
    if !is_quiet() {
        println!();
    }
}

/// All output goes through one mutex so result lines and the status line never interleave.
pub struct Progress {
    state: Mutex<State>,
    total: usize,
    live: bool,
}

#[derive(Default)]
struct State {
    done: usize,
    running: Vec<String>,
}

impl Progress {
    pub fn new(total: usize) -> Self {
        Self::with_live(total, !is_quiet() && std::io::stdout().is_terminal())
    }

    fn with_live(total: usize, live: bool) -> Self {
        Self {
            state: Mutex::default(),
            total,
            live,
        }
    }

    pub fn start(&self, name: &str) {
        let mut st = self.state.lock().unwrap();
        st.running.push(name.to_string());
        self.redraw(&st);
    }

    pub fn finish(&self, name: &str, print: impl FnOnce(&str)) {
        let mut st = self.state.lock().unwrap();
        if let Some(i) = st.running.iter().position(|n| n == name) {
            st.running.remove(i);
        }
        st.done += 1;
        self.clear();
        print(&format!("[{}/{}]", st.done, self.total));
        self.redraw(&st);
    }

    fn clear(&self) {
        if self.live {
            print!("\r\x1b[K");
        }
    }

    fn redraw(&self, st: &State) {
        if !self.live {
            return;
        }
        self.clear();
        if !st.running.is_empty() {
            let width = std::env::var("COLUMNS")
                .ok()
                .and_then(|c| c.parse().ok())
                .unwrap_or(80);
            print!("{}", gray(&status_line(st, self.total, width)));
        }
        let _ = std::io::stdout().flush();
    }
}

/// A wrapped status line cannot be erased with \r, so it is cut to fit one row.
fn status_line(st: &State, total: usize, width: usize) -> String {
    let line = format!("  ⟳ [{}/{}] {}", st.done, total, st.running.join(", "));
    let max = width.saturating_sub(1);
    if line.chars().count() > max {
        line.chars()
            .take(max.saturating_sub(1))
            .chain(['…'])
            .collect()
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_counts_finished_jobs() {
        let p = Progress::with_live(3, false);
        let mut seen = Vec::new();
        for name in ["a", "b", "c"] {
            p.start(name);
        }
        p.finish("b", |c| seen.push(c.to_string()));
        assert_eq!(p.state.lock().unwrap().running, ["a", "c"]);
        p.finish("a", |c| seen.push(c.to_string()));
        p.finish("c", |c| seen.push(c.to_string()));
        assert_eq!(seen, ["[1/3]", "[2/3]", "[3/3]"]);
        assert!(p.state.lock().unwrap().running.is_empty());
    }

    #[test]
    fn status_line_fits_one_row() {
        let st = State {
            done: 2,
            running: vec!["owner/a".into(), "owner/b".into()],
        };
        assert_eq!(status_line(&st, 9, 80), "  ⟳ [2/9] owner/a, owner/b");
        let cut = status_line(&st, 9, 16);
        assert_eq!(cut.chars().count(), 15);
        assert!(cut.ends_with('…'), "{cut}");
    }
}
