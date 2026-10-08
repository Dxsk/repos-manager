#!/bin/sh
# Render the AUR packages of a published GitHub release into dist/aur/, and
# optionally push them to the AUR (needs an AUR account with your SSH key).
#
#   packaging/aur/render.sh 1.0.0
#   packaging/aur/render.sh 1.0.0 --publish
set -eu

PKG=repos-manager
GH_REPO=Dxsk/repos-manager
ASSET=repos-manager

version=${1:?usage: render.sh <version> [--publish]}
version=${version#v}
publish=${2:-}

here=$(cd "$(dirname "$0")" && pwd)
out="$here/../../dist/aur"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

base="https://github.com/$GH_REPO"
curl -fsSL "$base/releases/download/v$version/SHA256SUMS" -o "$tmp/SHA256SUMS"
sum() {
    s=$(awk -v f="$1" '{ n = $2; sub(/^\*/, "", n) } n == f { print $1 }' "$tmp/SHA256SUMS")
    [ -n "$s" ] || { echo "$1 is not in SHA256SUMS" >&2; exit 1; }
    echo "$s"
}
sha_x86_64=$(sum "$ASSET-x86_64-unknown-linux-musl.tar.gz")
sha_aarch64=$(sum "$ASSET-aarch64-unknown-linux-musl.tar.gz")

# GitHub serves stable tag archives, but they are not in SHA256SUMS.
curl -fsSL "$base/archive/refs/tags/v$version.tar.gz" -o "$tmp/src.tar.gz"
if command -v sha256sum >/dev/null 2>&1; then
    sha_src=$(sha256sum "$tmp/src.tar.gz" | cut -d' ' -f1)
else
    sha_src=$(shasum -a 256 "$tmp/src.tar.gz" | cut -d' ' -f1)
fi

render() {
    sed -e "s/@VERSION@/$version/g" -e "s/@SHA_SRC@/$sha_src/g" \
        -e "s/@SHA_X86_64@/$sha_x86_64/g" -e "s/@SHA_AARCH64@/$sha_aarch64/g" "$1"
}

for pkg in "$PKG" "$PKG-bin"; do
    mkdir -p "$out/$pkg"
    render "$here/$pkg/PKGBUILD.in" > "$out/$pkg/PKGBUILD"
    render "$here/$pkg/SRCINFO.in" > "$out/$pkg/.SRCINFO"
    echo "Rendered $out/$pkg"
done

[ "$publish" = --publish ] || exit 0

for pkg in "$PKG" "$PKG-bin"; do
    git clone -q "ssh://aur@aur.archlinux.org/$pkg.git" "$tmp/$pkg"
    cp "$out/$pkg/PKGBUILD" "$out/$pkg/.SRCINFO" "$tmp/$pkg/"
    git -C "$tmp/$pkg" add PKGBUILD .SRCINFO
    if git -C "$tmp/$pkg" diff --cached --quiet; then
        echo "$pkg is already up to date on the AUR"
        continue
    fi
    git -C "$tmp/$pkg" commit -q -m "Update to $version"
    git -C "$tmp/$pkg" push -q origin HEAD:master
    echo "Published $pkg $version"
done
