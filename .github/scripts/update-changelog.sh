#!/usr/bin/env bash
# Prepend a new version section to CHANGELOG.md from commits since last tag.
#
# Usage: update-changelog.sh <new_version> <since_ref>
#   <new_version>  e.g. 0.4.0
#   <since_ref>    git ref to compute commits from (exclusive). Use "ROOT" to
#                  include the entire history (used when no prior tag exists).

set -euo pipefail

version="${1:?missing version}"
since="${2:?missing since ref}"

if [[ "$since" == "ROOT" ]]; then
    commits=$(git log --pretty=format:"%s")
else
    commits=$(git log "${since}..HEAD" --pretty=format:"%s")
fi

format_line() {
    local msg="$1"
    # Strip conventional-commit scope: "feat(foo):" -> "feat:"
    case "$msg" in
        feat*!:*|*"BREAKING CHANGE"*)
            echo "- ${msg}"
            ;;
        feat*:*)
            echo "- Add ${msg#*: }"
            ;;
        fix*:*)
            echo "- Fix ${msg#*: }"
            ;;
        # ignore chore/ci/docs/test/refactor/style noise
        *) ;;
    esac
}

entry=$(mktemp)
{
    echo "## v${version}"
    echo
    while IFS= read -r msg; do
        [[ -z "$msg" ]] && continue
        # Skip the bump commit itself
        [[ "$msg" == "chore: bump version"* ]] && continue
        format_line "$msg"
    done <<< "$commits"
    echo
} > "$entry"

# Bail out if we produced an empty section (header + blank lines only)
if [[ $(grep -c '^- ' "$entry") -eq 0 ]]; then
    echo "No user-facing changes found; leaving CHANGELOG.md untouched." >&2
    rm -f "$entry"
    exit 0
fi

# Insert after the "# Changelog" header, before the first existing "## " section
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
