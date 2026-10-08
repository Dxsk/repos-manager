#!/usr/bin/env bash
# Set the project version everywhere it is recorded.
#
# Usage: scripts/bump-version.sh <new_version>   (e.g. 1.2.0, no leading "v")

set -euo pipefail

version="${1:?usage: bump-version.sh <new_version>}"
version="${version#v}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] || {
    echo "invalid version: $version" >&2
    exit 1
}

cd "$(dirname "$0")/.."

# Edit only the [package] version, not dependency versions or other tables.
set_package_version() {
    local file="$1" header="$2" tmp
    tmp=$(mktemp)
    awk -v v="$version" -v header="$header" '
        /^\[/ { in_pkg = ($0 == header); is_self = 0 }
        in_pkg && $0 == "name = \"repos-manager\"" { is_self = 1 }
        in_pkg && is_self && !done && /^version = / {
            print "version = \"" v "\""; done = 1; next
        }
        { print }
    ' "$file" > "$tmp"
    cat "$tmp" > "$file"
    rm -f "$tmp"
}

set_package_version Cargo.toml "[package]"
# Patched directly instead of `cargo update -p repos-manager` so it works
# offline and without a Rust toolchain (e.g. in a minimal CI container).
[[ -f Cargo.lock ]] && set_package_version Cargo.lock "[[package]]"

sed -i.bak "s/\"version\": \"[^\"]*\"/\"version\": \"${version}\"/" site/src/_data/site.json
rm -f site/src/_data/site.json.bak

echo "Version set to ${version} in Cargo.toml, Cargo.lock and site/src/_data/site.json"
