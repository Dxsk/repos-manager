//! What a sync is about to do, said once before the first clone, plus the confirmation.

use std::fs;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::matcher::{CONFIG_FILE, RepoFilter, Rules};
use crate::output;

/// Pretend both streams are terminals, for the tty-less end-to-end tests only.
const FAKE_TTY: &str = "REPOS_MANAGER_FAKE_TTY";

#[derive(Debug, Clone)]
pub struct ScopeRepo {
    pub name: String,
    pub size_kb: Option<u64>,
    pub cloned: bool,
}

#[derive(Debug, Clone, Default)]
pub struct HostRow {
    pub host: String,
    pub repos: Vec<ScopeRepo>,
    /// Set only with `--prune` and a complete listing.
    pub prunable: Option<usize>,
    /// The listing lost entries, so `--prune` is off for this host.
    pub incomplete: bool,
}

impl HostRow {
    pub fn in_scope(&self) -> usize {
        self.repos.len()
    }

    fn to_clone(&self) -> impl Iterator<Item = &ScopeRepo> {
        self.repos.iter().filter(|r| !r.cloned)
    }

    fn clones(&self) -> usize {
        self.to_clone().count()
    }

    fn updates(&self) -> usize {
        self.repos.iter().filter(|r| r.cloned).count()
    }

    fn known_kb(&self) -> u64 {
        self.to_clone().filter_map(|r| r.size_kb).sum()
    }

    fn unknown(&self) -> usize {
        self.to_clone().filter(|r| r.size_kb.is_none()).count()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub hosts: Vec<HostRow>,
    pub prune: bool,
}

impl Plan {
    pub fn clones(&self) -> usize {
        self.hosts.iter().map(HostRow::clones).sum()
    }

    pub fn updates(&self) -> usize {
        self.hosts.iter().map(HostRow::updates).sum()
    }

    fn known_kb(&self) -> u64 {
        self.hosts.iter().map(HostRow::known_kb).sum()
    }

    fn unknown(&self) -> usize {
        self.hosts.iter().map(HostRow::unknown).sum()
    }

    fn has_work(&self) -> bool {
        self.clones() + self.updates() > 0
    }
}

/// A preference of the machine: lives in config.json, not the workspace's `.repos.yml`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConfirmSync {
    #[default]
    Auto,
    Always,
    /// Never ask. A big run is still summarized.
    Never,
}

impl ConfirmSync {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

pub const DEFAULT_NEW_REPOS: usize = 5;
pub const DEFAULT_DOWNLOAD_GB: f64 = 1.0;

const KB_PER_GB: f64 = 1024.0 * 1024.0;

pub fn gb_to_kb(gb: f64) -> u64 {
    (gb.max(0.0) * KB_PER_GB) as u64
}

#[derive(Debug, Clone, Copy)]
pub struct Confirm {
    pub mode: ConfirmSync,
    pub new_repos: usize,
    pub download_kb: u64,
}

impl Default for Confirm {
    fn default() -> Self {
        Self {
            mode: ConfirmSync::default(),
            new_repos: DEFAULT_NEW_REPOS,
            download_kb: gb_to_kb(DEFAULT_DOWNLOAD_GB),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Say nothing: a daily sync has to look exactly as it always has.
    Silent,
    Tell,
    Confirm,
}

/// Over the whole run, not per host. Unknown sizes never hide a large sync: the count rule still applies.
fn over_threshold(plan: &Plan, c: &Confirm) -> bool {
    plan.has_work() && (plan.clones() >= c.new_repos || plan.known_kb() >= c.download_kb)
}

/// Pure, so it is testable without a terminal. `skip_prompt` covers `-y` and `--dry-run`.
pub fn decide(plan: &Plan, c: &Confirm, interactive: bool, skip_prompt: bool) -> Ask {
    let worth_saying = match c.mode {
        ConfirmSync::Always => plan.has_work(),
        _ => over_threshold(plan, c),
    };
    if !worth_saying {
        return Ask::Silent;
    }
    if interactive && !skip_prompt && c.mode != ConfirmSync::Never {
        Ask::Confirm
    } else {
        Ask::Tell
    }
}

/// Both streams have to be terminals: a question nobody can answer would hang
/// a cron job or a CI step forever.
fn interactive() -> bool {
    if std::env::var_os(FAKE_TTY).is_some() {
        return true;
    }
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

pub fn human_kb(kb: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut value = kb as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    // A decimal only where it carries something: 1.2 GB, but 900 MB.
    if unit > 0 && value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{} {}", value.round() as u64, UNITS[unit])
    }
}

fn count(n: usize, word: &str) -> String {
    if n == 1 {
        format!("{n} {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// The volume of the new repos only: an update transfers little.
fn volume(h: &HostRow) -> Option<String> {
    let unknown = h.unknown();
    match (h.clones(), h.clones() - unknown) {
        (0, _) => None,
        // Not one new repo reported a size, and a figure here would be a guess.
        (_, 0) => Some("?".to_string()),
        _ if unknown == 0 => Some(human_kb(h.known_kb())),
        _ => Some(format!("{}, {unknown} unknown", human_kb(h.known_kb()))),
    }
}

fn scope_line(h: &HostRow) -> String {
    let mut line = format!(
        "{} in scope, {} cloned, {} new",
        h.in_scope(),
        h.updates(),
        h.clones()
    );
    if let Some(volume) = volume(h) {
        line.push_str(&format!(" ({volume})"));
    }
    line
}

fn biggest_new(h: &HostRow) -> Option<String> {
    let mut known: Vec<(&str, u64)> = h
        .to_clone()
        .filter_map(|r| r.size_kb.map(|kb| (r.name.as_str(), kb)))
        .collect();
    if known.is_empty() {
        return None;
    }
    known.sort_by_key(|(name, kb)| (std::cmp::Reverse(*kb), *name));
    let named: Vec<String> = known
        .iter()
        .take(2)
        .map(|(name, kb)| format!("{name} {}", human_kb(*kb)))
        .collect();
    Some(format!("biggest new: {}", named.join(", ")))
}

fn prune_note(h: &HostRow, prune: bool) -> Option<String> {
    if !prune {
        return None;
    }
    if h.incomplete {
        return Some(
            "with --prune: skipped, the repository list of this host is incomplete".to_string(),
        );
    }
    match h.prunable {
        Some(n) if n > 0 => Some(format!(
            "with --prune: {} would be deleted",
            count(n, "local repo")
        )),
        _ => None,
    }
}

/// "about": the APIs report a server side repository size, not the transfer size.
/// A size the provider does not report is counted as unknown, never estimated.
fn total_line(plan: &Plan) -> String {
    let volume = match (plan.known_kb(), plan.unknown()) {
        (0, 0) => String::new(),
        (0, _) => ", unknown download volume".to_string(),
        (kb, 0) => format!(", about {} to download", human_kb(kb)),
        (kb, n) => format!(", about {} to download, {n} of unknown size", human_kb(kb)),
    };
    format!(
        "{}, {}{volume}.",
        count(plan.clones(), "clone"),
        count(plan.updates(), "update")
    )
}

pub fn render(plan: &Plan) -> String {
    let width = plan
        .hosts
        .iter()
        .map(|h| h.host.chars().count())
        .max()
        .unwrap_or(0)
        + 2;
    let mut lines = Vec::new();
    for h in &plan.hosts {
        lines.push(format!("{:<width$}{}", h.host, scope_line(h)));
        lines.extend(
            [biggest_new(h), prune_note(h, plan.prune)]
                .into_iter()
                .flatten()
                .map(|note| format!("{:<width$}{note}", "")),
        );
    }
    lines.push(String::new());
    lines.push(total_line(plan));
    lines.join("\n")
}

pub fn narrowing_hint(base_dir: &Path) -> String {
    format!(
        "Nothing was cloned. Narrow the scope with --filter, --ignore or {}",
        output::path(base_dir.join(CONFIG_FILE))
    )
}

/// The reader is injected so the declined path is testable without a terminal.
pub fn confirm(plan: &Plan, input: &mut impl BufRead) -> bool {
    print!("\n{}\nProceed? [Y/n] ", render(plan));
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    // The end of the input (Ctrl-D) is not an answer, and not consent.
    if input.read_line(&mut answer).unwrap_or(0) == 0 {
        println!();
        return false;
    }
    matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "" | "y" | "yes"
    )
}

/// `false` means the user said no and nothing must run.
pub fn approved(plan: &Plan, c: &Confirm, skip_prompt: bool, base_dir: &Path) -> bool {
    match decide(plan, c, interactive(), skip_prompt) {
        Ask::Silent => true,
        Ask::Tell => {
            // -q hides an unasked-for summary, never the question below.
            if !output::is_quiet() {
                println!("\n{}\n", render(plan));
            }
            true
        }
        Ask::Confirm => {
            if confirm(plan, &mut std::io::stdin().lock()) {
                return true;
            }
            println!("{}", narrowing_hint(base_dir));
            false
        }
    }
}

/// Single quotes, with an inner `'` doubled. Not cosmetic: a Sourcehut name
/// starts with `~` and a glob can start with `*`, and neither is valid as a
/// bare YAML scalar.
fn quoted(pattern: &str) -> String {
    format!("'{}'", pattern.replace('\'', "''"))
}

/// A host is a mapping key, and one with a port holds a `:`.
fn host_key(host: &str) -> String {
    if host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
    {
        host.to_string()
    } else {
        quoted(host)
    }
}

/// A rule list as it already stands, uncommented. Nothing for an empty list:
/// an `only: []` is dropped on load, so writing one back would widen the scope.
fn rule_lines(patterns: &[String], key: &str, indent: usize) -> Vec<String> {
    if patterns.is_empty() {
        return Vec::new();
    }
    let pad = " ".repeat(indent);
    let mut lines = vec![format!("{pad}{key}:")];
    lines.extend(patterns.iter().map(|p| format!("{pad}  - {}", quoted(p))));
    lines
}

/// The rules already in force, then the commented catalogue under the same `ignore` key, biggest first.
fn host_block(row: &HostRow, rules: Rules<'_>) -> Vec<String> {
    let mut lines = vec![format!("  {}:", host_key(&row.host))];
    lines.extend(rule_lines(rules.only, "only", 4));
    lines.push("    ignore:".to_string());
    lines.extend(
        rules
            .ignore
            .iter()
            .map(|p| format!("      - {}", quoted(p))),
    );

    let mut repos: Vec<&ScopeRepo> = row.repos.iter().collect();
    // Reverse over the Option sorts the biggest first and the unknown last.
    repos.sort_by(|a, b| {
        (std::cmp::Reverse(a.size_kb), &a.name).cmp(&(std::cmp::Reverse(b.size_kb), &b.name))
    });
    let entries: Vec<(String, String)> = repos
        .iter()
        .map(|r| {
            (
                quoted(&r.name),
                r.size_kb.map_or_else(|| "?".to_string(), human_kb),
            )
        })
        .collect();
    const SIZE_COLUMN: usize = 24;
    let width = entries
        .iter()
        .map(|(p, _)| p.chars().count())
        .max()
        .unwrap_or(0)
        .max(SIZE_COLUMN);
    lines.extend(
        entries
            .iter()
            .map(|(pattern, size)| format!("      # - {pattern:<width$} # {size}")),
    );
    lines
}

/// The `--write-ignore` file: every repository in scope, commented out. It emits
/// `ignore` lists only, never an invented `only`, so it cannot empty the scope.
pub fn ignore_file(plan: &Plan, filter: &RepoFilter, today: &str) -> String {
    let mut lines = vec![
        format!("# Written by repos-manager on {today}."),
        "# Uncomment a repository to stop syncing it. Sizes are what the forge reports."
            .to_string(),
    ];
    let global = filter.global_rules();
    let carried = [
        rule_lines(global.only, "only", 0),
        rule_lines(global.ignore, "ignore", 0),
    ]
    .concat();
    if !carried.is_empty() {
        lines.push(String::new());
        lines.extend(carried);
    }
    if !plan.hosts.is_empty() {
        lines.push(String::new());
        lines.push("hosts:".to_string());
        for row in &plan.hosts {
            lines.extend(host_block(row, filter.host_rules(&row.host)));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

pub fn write_ignore_file(base_dir: &Path, content: &str, force: bool) -> Result<PathBuf> {
    let path = base_dir.join(CONFIG_FILE);
    if path.exists() && !force {
        bail!(
            "{} already exists, add --force to overwrite it",
            output::path(&path)
        );
    }
    // Same guard as prune: base_dir comes from the config or a flag.
    let base = base_dir
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", output::path(base_dir)))?;
    let parent = path.parent().unwrap_or(base_dir).canonicalize()?;
    if !parent.starts_with(&base) {
        bail!(
            "aborted: {} is outside the base directory",
            output::path(&path)
        );
    }
    fs::write(&path, content).with_context(|| format!("cannot write {}", output::path(&path)))?;
    Ok(path)
}

pub fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400);
    civil_date(days as i64)
}

/// Days since the epoch as `YYYY-MM-DD`, without a date dependency: Howard
/// Hinnant's civil_from_days.
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(name: &str, size_kb: Option<u64>) -> ScopeRepo {
        ScopeRepo {
            name: name.to_string(),
            size_kb,
            cloned: false,
        }
    }

    /// `cloned` repos stand in for the updates: only their count shows in the
    /// summary, while the generated file lists them like any other.
    fn row(host: &str, cloned: usize, to_clone: Vec<ScopeRepo>) -> HostRow {
        let mut repos: Vec<ScopeRepo> = (0..cloned)
            .map(|i| ScopeRepo {
                cloned: true,
                ..new(&format!("on-disk/c{i}"), Some(1))
            })
            .collect();
        repos.extend(to_clone);
        HostRow {
            host: host.to_string(),
            repos,
            ..Default::default()
        }
    }

    fn two_hosts() -> Plan {
        let mut small: Vec<ScopeRepo> = (0..14)
            .map(|i| new(&format!("team/s{i}"), Some(54_000)))
            .collect();
        small.push(new("team/monorepo", Some(3_250_586)));
        small.push(new("team/assets", Some(921_600)));
        small.push(new("team/mystery", None));
        small.push(new("team/riddle", None));
        let corp = HostRow {
            prunable: Some(2),
            ..row("gitlab.corp", 0, small)
        };
        let github = row(
            "github.com",
            39,
            vec![
                new("org/a", Some(600_000)),
                new("org/b", Some(600_000)),
                new("org/c", Some(58_291)),
            ],
        );
        Plan {
            hosts: vec![github, corp],
            prune: true,
        }
    }

    #[test]
    fn sizes_read_like_a_person_would_write_them() {
        let cases = [
            (0, "0 KB"),
            (512, "512 KB"),
            (1024, "1.0 MB"),
            (921_600, "900 MB"),
            (1_258_291, "1.2 GB"),
            (3_250_586, "3.1 GB"),
            (6_180_477, "5.9 GB"),
            (3 * 1024 * 1024 * 1024, "3.0 TB"),
        ];
        for (kb, expected) in cases {
            assert_eq!(human_kb(kb), expected, "{kb}");
        }
    }

    #[test]
    fn the_summary_names_the_scope_the_volume_and_the_prune() {
        assert_eq!(
            render(&two_hosts()),
            "\
github.com   42 in scope, 39 cloned, 3 new (1.2 GB)
             biggest new: org/a 586 MB, org/b 586 MB
gitlab.corp  18 in scope, 0 cloned, 18 new (4.7 GB, 2 unknown)
             biggest new: team/monorepo 3.1 GB, team/assets 900 MB
             with --prune: 2 local repos would be deleted

21 clones, 39 updates, about 5.9 GB to download, 2 of unknown size."
        );
    }

    /// A provider that reports no size at all must not look like an empty one.
    #[test]
    fn a_host_with_only_unknown_sizes_shows_a_question_mark() {
        let plan = Plan {
            hosts: vec![row(
                "git.example.org",
                1,
                vec![new("u/a", None), new("u/b", None)],
            )],
            prune: false,
        };
        assert_eq!(
            render(&plan),
            "\
git.example.org  3 in scope, 1 cloned, 2 new (?)

2 clones, 1 update, unknown download volume."
        );
    }

    /// `--prune` is already off for an incomplete listing, and the summary has
    /// to say so before the question, not after the run.
    #[test]
    fn an_incomplete_listing_is_named_before_the_question() {
        let plan = Plan {
            hosts: vec![HostRow {
                incomplete: true,
                ..row("gitlab.corp", 4, vec![new("u/a", Some(2048))])
            }],
            prune: true,
        };
        let out = render(&plan);
        assert!(
            out.contains("with --prune: skipped, the repository list of this host is incomplete"),
            "{out}"
        );
        assert!(!out.contains("would be deleted"), "{out}");
    }

    #[test]
    fn nothing_is_announced_when_a_run_is_small() {
        let c = Confirm::default();
        let quiet = Plan {
            hosts: vec![row("github.com", 57, vec![new("u/a", Some(1024))])],
            prune: false,
        };
        // A daily sync: 1 new repo, 1 MB. Interactive or not, not a word.
        assert_eq!(decide(&quiet, &c, true, false), Ask::Silent);
        assert_eq!(decide(&quiet, &c, false, false), Ask::Silent);
        // Nothing to do at all, whatever the mode.
        let idle = Plan::default();
        for mode in [ConfirmSync::Auto, ConfirmSync::Always, ConfirmSync::Never] {
            let c = Confirm { mode, ..c };
            assert_eq!(decide(&idle, &c, true, false), Ask::Silent, "{mode:?}");
        }
    }

    #[test]
    fn the_thresholds_decide_once_for_the_whole_run() {
        let c = Confirm::default();
        // Three new repos on each of two hosts: the count is run-wide.
        let spread = |n: usize| Plan {
            hosts: vec![
                row(
                    "a",
                    0,
                    (0..n).map(|i| new(&format!("u/{i}"), Some(1))).collect(),
                ),
                row(
                    "b",
                    0,
                    (0..n).map(|i| new(&format!("v/{i}"), Some(1))).collect(),
                ),
            ],
            prune: false,
        };
        assert_eq!(decide(&spread(3), &c, true, false), Ask::Confirm);
        assert_eq!(decide(&spread(2), &c, true, false), Ask::Silent);

        // One repo, over a gigabyte.
        let heavy = Plan {
            hosts: vec![row("a", 0, vec![new("u/big", Some(gb_to_kb(1.5)))])],
            prune: false,
        };
        assert_eq!(decide(&heavy, &c, true, false), Ask::Confirm);
        // Under the configured gigabytes, and under the count.
        let raised = Confirm {
            download_kb: gb_to_kb(2.0),
            ..c
        };
        assert_eq!(decide(&heavy, &raised, true, false), Ask::Silent);
    }

    #[test]
    fn a_prompt_needs_a_terminal_and_no_shortcut() {
        let c = Confirm::default();
        let big = Plan {
            hosts: vec![row("a", 0, vec![new("u/big", Some(gb_to_kb(4.0)))])],
            prune: false,
        };
        // Cron and CI: the summary is printed, the run goes on.
        assert_eq!(decide(&big, &c, false, false), Ask::Tell);
        // -y and --dry-run.
        assert_eq!(decide(&big, &c, true, true), Ask::Tell);
        // never still tells, always asks as soon as there is anything to do.
        let never = Confirm {
            mode: ConfirmSync::Never,
            ..c
        };
        assert_eq!(decide(&big, &never, true, false), Ask::Tell);
        let always = Confirm {
            mode: ConfirmSync::Always,
            ..c
        };
        let one_update = Plan {
            hosts: vec![row("a", 1, vec![])],
            prune: false,
        };
        assert_eq!(decide(&one_update, &always, true, false), Ask::Confirm);
        assert_eq!(decide(&one_update, &always, false, false), Ask::Tell);
    }

    #[test]
    fn the_answer_defaults_to_yes() {
        let plan = two_hosts();
        for yes in ["", "\n", "y\n", "Y\n", "yes\n", "  \n"] {
            let mut input = yes.as_bytes();
            assert_eq!(confirm(&plan, &mut input), !yes.is_empty(), "{yes:?}");
        }
        for no in ["n\n", "N\n", "no\n", "nope\n", "q\n"] {
            let mut input = no.as_bytes();
            assert!(!confirm(&plan, &mut input), "{no:?}");
        }
    }

    #[test]
    fn modes_are_read_from_their_config_spelling() {
        assert_eq!(ConfirmSync::parse("auto"), Some(ConfirmSync::Auto));
        assert_eq!(ConfirmSync::parse("always"), Some(ConfirmSync::Always));
        assert_eq!(ConfirmSync::parse("never"), Some(ConfirmSync::Never));
        assert_eq!(ConfirmSync::parse("Never"), None);
        assert_eq!(ConfirmSync::default(), ConfirmSync::Auto);
        assert_eq!(Confirm::default().new_repos, 5);
        assert_eq!(Confirm::default().download_kb, 1024 * 1024);
    }

    fn rules(dir: &Path, yaml: Option<&str>) -> RepoFilter {
        if let Some(text) = yaml {
            fs::write(dir.join(CONFIG_FILE), text).unwrap();
        }
        RepoFilter::load(dir, &[], &[]).unwrap()
    }

    #[test]
    fn the_generated_file_comments_out_every_repository_biggest_first() {
        let dir = tempfile::tempdir().unwrap();
        let plan = Plan {
            hosts: vec![
                row(
                    "github.com",
                    1,
                    vec![
                        new("Dxsk/notes", None),
                        new("Dxsk/assets", Some(921_600)),
                        new("Dxsk/monorepo", Some(3_250_586)),
                    ],
                ),
                row("git.sr.ht", 0, vec![new("~dxsk/dotfiles", Some(12_288))]),
            ],
            prune: false,
        };
        assert_eq!(
            ignore_file(&plan, &rules(dir.path(), None), "2026-10-11"),
            "\
# Written by repos-manager on 2026-10-11.
# Uncomment a repository to stop syncing it. Sizes are what the forge reports.

hosts:
  github.com:
    ignore:
      # - 'Dxsk/monorepo'          # 3.1 GB
      # - 'Dxsk/assets'            # 900 MB
      # - 'on-disk/c0'             # 1 KB
      # - 'Dxsk/notes'             # ?
  git.sr.ht:
    ignore:
      # - '~dxsk/dotfiles'         # 12 MB
"
        );
    }

    /// Dropping the rules in force would widen the scope, which is the one
    /// thing this file exists to prevent.
    #[test]
    fn a_rewrite_carries_over_the_rules_already_in_force() {
        let dir = tempfile::tempdir().unwrap();
        let existing = "\
only: [Dxsk/*]
ignore: ['test-org/*']
hosts:
  gitlab.corp:
    only: [team/*]
    ignore: [team/archive]
";
        let plan = Plan {
            hosts: vec![row(
                "gitlab.corp",
                0,
                vec![new("team/app", Some(2048)), new("team/o'hara", Some(512))],
            )],
            prune: false,
        };
        assert_eq!(
            ignore_file(&plan, &rules(dir.path(), Some(existing)), "2026-10-11"),
            "\
# Written by repos-manager on 2026-10-11.
# Uncomment a repository to stop syncing it. Sizes are what the forge reports.

only:
  - 'Dxsk/*'
ignore:
  - 'test-org/*'

hosts:
  gitlab.corp:
    only:
      - 'team/*'
    ignore:
      - 'team/archive'
      # - 'team/app'               # 2.0 MB
      # - 'team/o''hara'           # 512 KB
"
        );
    }

    #[test]
    fn patterns_and_host_keys_are_quoted_where_yaml_needs_it() {
        assert_eq!(quoted("~dxsk/dotfiles"), "'~dxsk/dotfiles'");
        assert_eq!(quoted("*/tmp-*"), "'*/tmp-*'");
        assert_eq!(quoted("o'hara/repo"), "'o''hara/repo'");
        assert_eq!(host_key("github.com"), "github.com");
        assert_eq!(host_key("git_example-org.net"), "git_example-org.net");
        assert_eq!(host_key("git.example.org:3000"), "'git.example.org:3000'");
    }

    /// A run with nothing to do is a valid answer, and `hosts:` with nothing
    /// under it would not load.
    #[test]
    fn nothing_in_the_plan_writes_no_hosts_key() {
        let dir = tempfile::tempdir().unwrap();
        let out = ignore_file(&Plan::default(), &rules(dir.path(), None), "2026-10-11");
        assert!(!out.contains("hosts:"), "{out}");
        assert!(out.ends_with("forge reports.\n"), "{out}");
    }

    #[test]
    fn the_file_is_never_overwritten_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_ignore_file(dir.path(), "first\n", false).unwrap();
        assert_eq!(path, dir.path().join(CONFIG_FILE));

        let err = write_ignore_file(dir.path(), "second\n", false)
            .unwrap_err()
            .to_string();
        assert!(err.contains(CONFIG_FILE), "{err}");
        assert!(err.contains("--force"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "first\n");

        write_ignore_file(dir.path(), "second\n", true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second\n");
    }

    #[test]
    fn the_header_carries_a_civil_date() {
        let cases = [
            (0, "1970-01-01"),
            (10_956, "1999-12-31"),
            (11_016, "2000-02-29"),
            (20_737, "2026-10-11"),
            (47_541, "2100-03-01"),
        ];
        for (days, expected) in cases {
            assert_eq!(civil_date(days), expected, "{days}");
        }
        assert_eq!(today().len(), "2026-10-11".len());
    }

    #[test]
    fn the_hint_names_every_way_to_sync_less() {
        let hint = narrowing_hint(Path::new("/base"));
        for part in ["--filter", "--ignore", ".repos.yml"] {
            assert!(hint.contains(part), "{hint}");
        }
    }
}
