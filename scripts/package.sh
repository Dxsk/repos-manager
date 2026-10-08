#!/usr/bin/env bash
# CI helper: pack a release build into dist/<asset> with LICENSE and readme.
#
# Usage: scripts/package.sh <rust-target> <asset-name.tar.gz|asset-name.zip>

set -euo pipefail

target="${1:?missing rust target}"
asset="${2:?missing asset name}"

cd "$(dirname "$0")/.."

bin=repos-manager
[[ "$target" == *windows* ]] && bin=repos-manager.exe

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp "target/$target/release/$bin" LICENSE readme.md "$stage/"

mkdir -p dist
out="$PWD/dist/$asset"
rm -f "$out"

case "$asset" in
    *.tar.gz) tar -czf "$out" -C "$stage" "$bin" LICENSE readme.md ;;
    *.zip)
        cd "$stage"
        if command -v zip >/dev/null 2>&1; then
            zip -q "$out" "$bin" LICENSE readme.md
        else
            7z a -tzip -bd "$out" "$bin" LICENSE readme.md >/dev/null
        fi
        ;;
    *) echo "unsupported archive type: $asset" >&2; exit 1 ;;
esac

echo "dist/$asset"
