#!/usr/bin/env python3
"""Per-function complexity budgets, measured with Mozilla's rust-code-analysis.

Run `make metrics`. The analyser is a separate binary:

    cargo install rust-code-analysis-cli --locked

It reports cyclomatic and cognitive complexity, exit points and length for
every function. This script keeps only production code, prints the worst
offenders, and fails when a function crosses its limit.

New code must fit BUDGETS. The functions in BASELINE were already over when the
check was added: each is capped at the value it had then, so the debt is named
in the diff and can only shrink. Going under a cap fails too, with the number
to write instead, which is what keeps the ratchet turning.

Test code is left out: a long table-driven test is not the kind of complexity
these budgets are about. Finding it relies on `cargo fmt`, which CI enforces:
a `#[cfg(test)]` item starts at column 0 and ends at the first later line that
is exactly `}`. The script fails if that assumption ever breaks.
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

ANALYSER = "rust-code-analysis-cli"

METRICS = ("cognitive", "cyclomatic", "sloc", "nexits")

# What new code has to fit in.
BUDGETS = {
    "cognitive": 25,  # nested branching, how hard the body is to follow
    "cyclomatic": 30,  # independent paths through the body
    "sloc": 130,  # lines, closures included
    "nexits": 10,  # return points
}

# Already over budget when this check landed (2026-10-10). Lower a number when
# you split one of these up; never raise one.
BASELINE = {
    "src/sync/mod.rs::sync_listed": {"cognitive": 15},
    "src/status.rs::scan": {"cognitive": 23},
}

CFG_TEST = re.compile(r"^#\[cfg\(test\)\]\s*$")
# `#[cfg(test)] mod foo;` makes the whole of src/foo.rs test code.
TEST_MOD_FILE = re.compile(r"^\s*(?:pub\s+)?mod\s+(\w+)\s*;")


def test_ranges(path: Path) -> tuple[list[tuple[int, int]], list[str]]:
    """Line ranges (1-based, inclusive) of the `#[cfg(test)]` items in a file,
    plus the names of the test-only modules it declares."""
    lines = path.read_text(encoding="utf-8").splitlines()
    ranges: list[tuple[int, int]] = []
    modules: list[str] = []
    for i, line in enumerate(lines):
        if not CFG_TEST.match(line):
            continue
        following = lines[i + 1] if i + 1 < len(lines) else ""
        if mod := TEST_MOD_FILE.match(following):
            modules.append(mod.group(1))
            continue
        end = next((j for j in range(i + 1, len(lines)) if lines[j] == "}"), None)
        if end is None:
            sys.exit(f"{path}:{i + 1}: cannot find the end of this cfg(test) item")
        ranges.append((i + 1, end + 1))
    return ranges, modules


def functions(space: dict, nested: bool = False):
    """Every function, with its closures folded into it rather than listed on
    their own: a closure is part of the body a reader has to hold."""
    if space["kind"] == "function" and not nested:
        yield space
    inside = nested or space["kind"] == "function"
    for child in space["spaces"]:
        yield from functions(child, inside)


def measure(path: Path) -> list[dict]:
    out = subprocess.run(
        [ANALYSER, "-m", "-p", str(path), "-O", "json", "--pr"],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if out.returncode != 0 or not out.stdout.strip():
        sys.exit(f"{ANALYSER} failed on {path}: {out.stderr.strip()}")
    skip, _ = test_ranges(path)
    rows = []
    for fn in functions(json.loads(out.stdout)):
        line = int(fn["start_line"])
        if any(lo <= line <= hi for lo, hi in skip):
            continue
        m = fn["metrics"]
        rows.append(
            {
                "key": f"{path.as_posix()}::{fn['name']}",
                "file": path.as_posix(),
                "line": line,
                "name": fn["name"] or "<anonymous>",
                "cognitive": int(m["cognitive"]["sum"]),
                "cyclomatic": int(m["cyclomatic"]["sum"]),
                "sloc": int(m["loc"]["sloc"]),
                "nexits": int(m["nexits"]["sum"]),
            }
        )
    return rows


def sources(root: Path) -> list[Path]:
    """Every Rust file except the ones a `#[cfg(test)] mod` brings in."""
    files = sorted(root.rglob("*.rs"))
    test_only = {
        f.parent / f"{m}.rs" for f in files for m in test_ranges(f)[1]
    }
    return [f for f in files if f not in test_only]


def limit(key: str, metric: str) -> int:
    return max(BUDGETS[metric], BASELINE.get(key, {}).get(metric, 0))


def main() -> int:
    if shutil.which(ANALYSER) is None:
        print(
            f"{ANALYSER} not found. Install it with:\n"
            f"  cargo install rust-code-analysis-cli --locked",
            file=sys.stderr,
        )
        return 2

    rows = [row for path in sources(Path("src")) for row in measure(path)]
    if not rows:
        print("no functions measured", file=sys.stderr)
        return 2

    over, stale = [], []
    for r in rows:
        for metric in METRICS:
            allowed = BASELINE.get(r["key"], {}).get(metric)
            if r[metric] > limit(r["key"], metric):
                over.append((r, metric))
            elif allowed is not None and r[metric] < allowed:
                stale.append((r, metric, allowed))

    rows.sort(key=lambda r: (-r["cognitive"], -r["cyclomatic"], r["file"], r["line"]))
    header = f"{'cog':>5} {'cyc':>5} {'sloc':>5} {'exits':>6}  function"
    print(f"{len(rows)} functions measured with {ANALYSER} (production code only)\n")
    print(header)
    print("-" * len(header))
    for r in rows[:15]:
        mark = " *" if r["key"] in BASELINE else ""
        print(
            f"{r['cognitive']:>5} {r['cyclomatic']:>5} {r['sloc']:>5} {r['nexits']:>6}"
            f"  {r['file']}:{r['line']} {r['name']}{mark}"
        )
    if any(r["key"] in BASELINE for r in rows[:15]):
        print("\n* capped at its own baseline, not at the budget")

    print(f"\n{'budget':<11}{'worst':>6}{'limit':>7}")
    for metric in METRICS:
        worst = max(r[metric] for r in rows)
        print(f"{metric:<11}{worst:>6}{BUDGETS[metric]:>7}")

    for r, metric, was in stale:
        print(
            f"\n{r['file']}:{r['line']} {r['name']}: {metric} is down to "
            f"{r[metric]} from {was}. Lower its BASELINE entry in "
            f"scripts/code-metrics.py to keep the gain.",
            file=sys.stderr,
        )
    for r, metric in sorted(over, key=lambda o: -o[0][o[1]]):
        print(
            f"\n{r['file']}:{r['line']} {r['name']}: {metric} {r[metric]} "
            f"over the limit of {limit(r['key'], metric)}. Split it up, or "
            f"justify it in scripts/code-metrics.py.",
            file=sys.stderr,
        )
    if over or stale:
        return 1

    print("\nevery function within its limit")
    return 0


if __name__ == "__main__":
    sys.exit(main())
