//! Reconnaissance: what a sync is about to do, said once before the first
//! clone, plus the single confirmation that follows when the run is big.
//!
//! The sizes come from the forge APIs, which report the size of the
//! repository on the server and not what the transfer will cost, so every
//! total says "about". A size the provider does not report is counted as
//! unknown and never estimated.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

/// Pretend both streams are terminals. For the end-to-end tests only, which
/// have no tty: it is not part of the documented interface.
const FAKE_TTY: &str = "REPOS_MANAGER_FAKE_TTY";

/// A repository the sync would clone.
#[derive(Debug, Clone)]
pub struct NewRepo {
    pub name: String,
    pub size_kb: Option<u64>,
}

/// One host's line in the summary.
#[derive(Debug, Clone, Default)]
pub struct HostRow {
    pub host: String,
    /// Repos in scope whose directory already holds a clone: the updates.
    pub cloned: usize,
    pub new: Vec<NewRepo>,
    /// Local repos `--prune` would consider deleting, set only when `--prune`
    /// was asked and this host's listing is complete.
    pub prunable: Option<usize>,
    /// The listing lost entries, so `--prune` is off for this host.
    pub incomplete: bool,
}

impl HostRow {
    /// What the filter kept for this host.
    pub fn in_scope(&self) -> usize {
        self.cloned + self.new.len()
    }

    /// Total size of the new repos whose size the provider reported.
    fn known_kb(&self) -> u64 {
        self.new.iter().filter_map(|r| r.size_kb).sum()
    }

    fn unknown(&self) -> usize {
        self.new.iter().filter(|r| r.size_kb.is_none()).count()
    }
}

/// What the whole run would do, every host included.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub hosts: Vec<HostRow>,
    /// `--prune` was asked for: the rows then say what it would delete.
    pub prune: bool,
}

impl Plan {
    pub fn clones(&self) -> usize {
        self.hosts.iter().map(|h| h.new.len()).sum()
    }

    pub fn updates(&self) -> usize {
        self.hosts.iter().map(|h| h.cloned).sum()
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

/// When a sync stops to ask. This is a preference of the machine, so it lives
/// in config.json next to `parallel`, not in the workspace's `.repos.yml`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConfirmSync {
    /// Ask only when the run crosses a threshold.
    #[default]
    Auto,
    /// Ask as soon as there is anything to do.
    Always,
    /// Never ask. A big run is still summarized, so the information is not lost.
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

/// The confirmation preferences: `confirm_sync`, `confirm_new_repos` and
/// `confirm_download_gb`.
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

/// What the run should do about its plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Say nothing: a daily sync has to look exactly as it always has.
    Silent,
    /// Print the summary and carry on.
    Tell,
    /// Print the summary and wait for an answer.
    Confirm,
}

/// Counted over the whole run, not per host. An unknown volume never hides a
/// large sync: the count rule applies whatever the sizes say.
fn over_threshold(plan: &Plan, c: &Confirm) -> bool {
    plan.has_work() && (plan.clones() >= c.new_repos || plan.known_kb() >= c.download_kb)
}

/// Pure, so the whole table can be tested without a terminal. `skip_prompt`
/// covers `-y` and `--dry-run`: both want the summary without the question.
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

/// A size as a person would write it, from the kibibytes the APIs report.
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
    match (h.new.len(), h.new.len() - unknown) {
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
        h.cloned,
        h.new.len()
    );
    if let Some(volume) = volume(h) {
        line.push_str(&format!(" ({volume})"));
    }
    line
}

/// The two largest new repos with a known size: what the volume is made of.
fn biggest_new(h: &HostRow) -> Option<String> {
    let mut known: Vec<(&str, u64)> = h
        .new
        .iter()
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

/// What `--prune` would do to this host, before anything is deleted.
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

/// "about" because the APIs report a repository size on the server, not the
/// size of the transfer.
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

/// The summary, as text: deciding and rendering stay apart so both can be
/// tested, and so step 4 can render the same plan elsewhere.
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

/// How to sync less, for the run that was just declined.
pub fn narrowing_hint(base_dir: &Path) -> String {
    format!(
        "Nothing was cloned. Narrow the scope with --filter, --ignore or {}",
        base_dir.join(".repos.yml").display()
    )
}

/// The summary and the one question of the whole run. The reader is injected
/// so the declined path can be tested without a terminal.
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

/// Decide, print, ask. `false` means the user said no and nothing must run.
pub fn approved(plan: &Plan, c: &Confirm, skip_prompt: bool, base_dir: &Path) -> bool {
    match decide(plan, c, interactive(), skip_prompt) {
        Ask::Silent => true,
        Ask::Tell => {
            println!("\n{}\n", render(plan));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn new(name: &str, size_kb: Option<u64>) -> NewRepo {
        NewRepo {
            name: name.to_string(),
            size_kb,
        }
    }

    fn row(host: &str, cloned: usize, new: Vec<NewRepo>) -> HostRow {
        HostRow {
            host: host.to_string(),
            cloned,
            new,
            ..Default::default()
        }
    }

    /// The two hosts of the feature's target output, with real figures.
    fn two_hosts() -> Plan {
        let mut small: Vec<NewRepo> = (0..14)
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

    #[test]
    fn the_hint_names_every_way_to_sync_less() {
        let hint = narrowing_hint(Path::new("/base"));
        for part in ["--filter", "--ignore", ".repos.yml"] {
            assert!(hint.contains(part), "{hint}");
        }
    }
}
