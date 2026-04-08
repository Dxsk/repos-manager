#!/usr/bin/env bash
# Prepend a new version section to CHANGELOG.md from commits since last tag.
#
# Usage: update-changelog.sh <new_version> <since_ref>
#   <new_version>  e.g. 0.4.0
#   <since_ref>    git ref to compute commits from (exclusive). Use "ROOT" to
#                  include the entire history (used when no prior tag exists).
#
# Behaviour notes
# ---------------
# - If CHANGELOG.md already has a "## v<new_version>" heading, the script is a
#   no-op. This prevents duplicate sections when the auto-version workflow
#   re-runs on a release that was already bumped manually.
# - Squash-merge PR commits do not carry a conventional-commit prefix on their
#   subject line, but GitHub includes the original per-commit subjects in the
#   commit body. We walk both the subject and body lines so a squash merge
#   still contributes its feat/fix entries to the changelog.
# - Commit bodies are read with %B (subject + body + trailers), split on
#   newlines, and every line is evaluated through format_line.

set -euo pipefail

version="${1:?missing version}"
since="${2:?missing since ref}"

# Short-circuit: heading already present, nothing to do.
if [[ -f CHANGELOG.md ]] && grep -qxF "## v${version}" CHANGELOG.md; then
    echo "CHANGELOG.md already has a v${version} section; leaving it untouched." >&2
    exit 0
fi

# %B prints the full commit message (subject + body). We need a
# between-commits separator that (a) never appears in a commit message
# and (b) survives bash command substitution, which strips NUL bytes.
# Use a fixed sentinel token surrounded by newlines; "<<<RM-COMMIT>>>"
# is vanishingly unlikely to appear in a real commit body.
sep='<<<RM-COMMIT-SEP>>>'
if [[ "$since" == "ROOT" ]]; then
    commit_blob=$(git log --pretty=format:"%B${sep}")
else
    commit_blob=$(git log "${since}..HEAD" --pretty=format:"%B${sep}")
fi

format_line() {
    local msg="$1"
    # Strip GitHub squash-merge bullet prefixes ("* ", "- ") and any
    # leading whitespace so "* feat(foo): bar" still matches the feat
    # pattern below.
    msg="${msg#"${msg%%[![:space:]]*}"}"
    msg="${msg#\* }"
    msg="${msg#- }"
    msg="${msg#"${msg%%[![:space:]]*}"}"

    local body
    case "$msg" in
        feat*!:*|*"BREAKING CHANGE"*)
            echo "- ${msg}"
            return 0
            ;;
        feat*:*)
            body="${msg#*: }"
            # Avoid "Add add foo" when the commit already starts with "add".
            case "$body" in
                [Aa]dd\ *|[Aa]dds\ *|[Aa]dded\ *) echo "- ${body^}" ;;
                *)                                echo "- Add ${body}" ;;
            esac
            return 0
            ;;
        fix*:*)
            body="${msg#*: }"
            case "$body" in
                [Ff]ix\ *|[Ff]ixes\ *|[Ff]ixed\ *) echo "- ${body^}" ;;
                *)                                 echo "- Fix ${body}" ;;
            esac
            return 0
            ;;
        # Ignore chore/ci/docs/test/refactor/style noise.
        *) ;;
    esac
}

entry=$(mktemp)
seen=$(mktemp)
{
    echo "## v${version}"
    echo
    # Walk every line of the aggregated commit blob. Lines containing
    # only the sentinel separator are commit boundaries and are skipped.
    # Parsing line-by-line across the whole blob works because format_line
    # is a pure per-line filter: only lines that look like a conventional
    # commit prefix contribute to the changelog, so interleaving is fine.
    while IFS= read -r msg; do
        [[ -z "$msg" ]] && continue
        [[ "$msg" == "$sep" ]] && continue
        # Skip the bump commit itself.
        [[ "$msg" == "chore: bump version"* ]] && continue
        line=$(format_line "$msg")
        [[ -z "$line" ]] && continue
        # Deduplicate identical lines coming from both the merge subject
        # and the original commits carried in the merge body.
        if ! grep -qxF -- "$line" "$seen"; then
            echo "$line" >> "$seen"
            echo "$line"
        fi
    done <<< "$commit_blob"
    echo
} > "$entry"

rm -f "$seen"

# Bail out if we produced an empty section (header + blank lines only).
if [[ $(grep -c '^- ' "$entry") -eq 0 ]]; then
    echo "No user-facing changes found; leaving CHANGELOG.md untouched." >&2
    rm -f "$entry"
    exit 0
fi

# Insert after the "# Changelog" header, before the first existing "## "
# section.
new=$(mktemp)
awk -v entry_file="$entry" '
    !inserted && /^## / {
        while ((getline line < entry_file) > 0) print line
        inserted=1
    }
    { print }
    END {
        if (!inserted) {
            while ((getline line < entry_file) > 0) print line
        }
    }
' CHANGELOG.md > "$new"

mv "$new" CHANGELOG.md
rm -f "$entry"
echo "CHANGELOG.md updated for v${version}"
