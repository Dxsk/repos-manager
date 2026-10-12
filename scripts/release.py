#!/usr/bin/env python3
"""Cut a release: bump the version, date the changelog, commit and tag.

    python scripts/release.py 1.2.0

`make release V=1.2.0` runs this. Call it directly where make is not
available, which is every Windows checkout.

Nothing is pushed: check the result, then push the branch, main and the tag in
ONE command, as the final line tells you. The forge mirrors to GitHub on every
push, so a tag that travels alone leaves main behind until the next scheduled
sync, and GitHub builds the release while the site still serves the old version.
"""

from __future__ import annotations

import re
import subprocess
import sys
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Each file holds the version once, in this shape.
VERSION_FILES = (
    ("Cargo.toml", r'^version = ".*"', 'version = "{v}"'),
    ("site/src/_data/site.json", r'"version": ".*"', '"version": "{v}"'),
    ("repos-manager.sh", r'^VERSION=".*"', 'VERSION="{v}"'),
)
CHANGELOG = "CHANGELOG.md"
UNRELEASED = "^## Unreleased"


def run(*args: str, hint: str = "") -> None:
    """Run a command in the repository, aborting on failure."""
    # Flushed first: our own output is buffered when piped, the child's is not,
    # so without this the steps print out of order.
    sys.stdout.flush()
    if subprocess.run(args, cwd=ROOT).returncode != 0:
        sys.exit(f"failed: {' '.join(args)}\n{hint}" if hint else f"failed: {' '.join(args)}")


def git_out(*args: str) -> str:
    """Git output, empty when the command has nothing to say or fails."""
    out = subprocess.run(
        ("git", *args), cwd=ROOT, capture_output=True, text=True, encoding="utf-8"
    )
    return out.stdout.strip() if out.returncode == 0 else ""


def identity() -> tuple[str, str]:
    """Who the release commit is attributed to.

    The configured identity when there is one. This repository is often used
    with none, so the author of HEAD stands in rather than failing: a release
    is not the moment to discover an unset `user.email`.
    """
    name, email = git_out("config", "user.name"), git_out("config", "user.email")
    if name and email:
        return name, email
    name = name or git_out("log", "-1", "--format=%an")
    email = email or git_out("log", "-1", "--format=%ae")
    if not (name and email):
        sys.exit("no git identity and no commit to borrow one from")
    print(f"no configured git identity, using the author of HEAD: {name} <{email}>")
    return name, email


def check(version: str) -> None:
    """Everything that must hold before a single file is touched."""
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        sys.exit(f"version must be x.y.z, got {version!r}")
    if git_out("status", "--porcelain"):
        sys.exit("commit your changes first")
    if git_out("tag", "--list", f"v{version}"):
        sys.exit(f"tag v{version} already exists")
    changelog = (ROOT / CHANGELOG).read_text(encoding="utf-8")
    if not re.search(UNRELEASED, changelog, re.M):
        sys.exit(f"{CHANGELOG} has no '## Unreleased' section")
    for name, pattern, _ in VERSION_FILES:
        text = (ROOT / name).read_text(encoding="utf-8")
        if len(re.findall(pattern, text, re.M)) != 1:
            sys.exit(f"{name}: expected exactly one {pattern!r}")


def rewrite(version: str) -> list[str]:
    """Set the version everywhere and date the changelog. Returns the files."""
    edits = [(n, p, r.format(v=version)) for n, p, r in VERSION_FILES]
    edits.append((CHANGELOG, UNRELEASED, f"## {version} ({date.today().isoformat()})"))
    for name, pattern, replacement in edits:
        path = ROOT / name
        text, n = re.subn(
            pattern, replacement, path.read_text(encoding="utf-8"), count=1, flags=re.M
        )
        if n != 1:
            sys.exit(f"{name}: {pattern!r} matched {n} times")
        # newline="\n" because .gitattributes keeps this tree LF on every OS.
        path.write_text(text, encoding="utf-8", newline="\n")
        print(f"  {name}")
    return [name for name, _, _ in edits]


def main() -> None:
    if len(sys.argv) != 2 or sys.argv[1] in ("-h", "--help"):
        sys.exit(__doc__)
    version = sys.argv[1].lstrip("v")
    check(version)

    print(f"setting version {version}")
    files = rewrite(version)
    run("cargo", "update", "--workspace", "--offline")

    name, email = identity()
    who = ("-c", f"user.name={name}", "-c", f"user.email={email}")
    run("git", *who, "commit", "-m", f"chore: release v{version}", "Cargo.lock", *files)
    tag = f"v{version}"
    # The tag is signed, so GPG has to be able to sign for this identity
    # without a prompt. The release commit is already made by then.
    run(
        "git", *who, "tag", "-s", tag, "-m", tag,
        hint=f"the release commit is in place, so once GPG can sign for "
        f"{email}, all that is left is:\n"
        f"  git -c user.name={name} -c user.email={email} tag -s {tag} -m {tag}",
    )

    # One push, so the mirror carries the branch and the tag together.
    branch = git_out("rev-parse", "--abbrev-ref", "HEAD") or "dev"
    print(f"\ncommitted and tagged {tag}. Now run, as a single push:")
    print(f"  git push origin {branch} {branch}:main {tag}")


if __name__ == "__main__":
    main()
