use std::io::IsTerminal;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

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
