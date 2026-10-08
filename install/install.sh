#!/bin/sh
# repos-manager installer for Linux and macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --version v1.0.0 --prefix /usr/local

set -eu

NAME=repos-manager
GITHUB_REPO=https://github.com/Dxsk/repos-manager
FORGE_REPO=https://forge.infrasouveraine.fr/dxsk/repos-manager
FORGE_API=https://forge.infrasouveraine.fr/api/v1/repos/dxsk/repos-manager

version="${REPOS_MANAGER_VERSION:-}"
prefix="${PREFIX:-$HOME/.local}"
source_name=github
uninstall=0

say() { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

usage() {
    cat <<'USAGE'
Usage: install.sh [options]

  --version vX.Y.Z   install a specific release (default: latest, or $REPOS_MANAGER_VERSION)
  --prefix DIR       install into DIR/bin (default: $PREFIX or ~/.local)
  --source SRC       download from 'github' (default) or 'forge'
  --uninstall        remove the installed binary
  -h, --help         show this help
USAGE
}

while [ $# -gt 0 ]; do
    case "$1" in
        --version) [ $# -ge 2 ] || die "--version needs a value"; version="$2"; shift 2 ;;
        --version=*) version="${1#*=}"; shift ;;
        --prefix) [ $# -ge 2 ] || die "--prefix needs a value"; prefix="$2"; shift 2 ;;
        --prefix=*) prefix="${1#*=}"; shift ;;
        --source) [ $# -ge 2 ] || die "--source needs a value"; source_name="$2"; shift 2 ;;
        --source=*) source_name="${1#*=}"; shift ;;
        --uninstall) uninstall=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown option: $1" ;;
    esac
done

bindir="$prefix/bin"
target="$bindir/$NAME"

if [ "$uninstall" -eq 1 ]; then
    if [ -e "$target" ]; then
        rm -f "$target"
        say "Removed $target"
    else
        say "Nothing to remove at $target"
    fi
    exit 0
fi

case "$source_name" in
    github|forge) ;;
    *) die "--source must be 'github' or 'forge'" ;;
esac

if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL --retry 3 -o "$2" "$1"; }
    fetch_stdout() { curl -fsSL --retry 3 "$1"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -q -O "$2" "$1"; }
    fetch_stdout() { wget -q -O - "$1"; }
else
    die "curl or wget is required"
fi

case "$(uname -s)" in
    Linux) os=linux ;;
    Darwin) os=macos ;;
    *) die "unsupported OS: $(uname -s) (on Windows use install.ps1)" ;;
esac

case "$(uname -m)" in
    x86_64|amd64) arch=x86_64 ;;
    aarch64|arm64) arch=aarch64 ;;
    *) die "unsupported architecture: $(uname -m)" ;;
esac

asset="$NAME-$os-$arch.tar.gz"

if [ -n "$version" ]; then
    case "$version" in v*) ;; *) version="v$version" ;; esac
fi

if [ "$source_name" = forge ]; then
    if [ -z "$version" ]; then
        version=$(fetch_stdout "$FORGE_API/releases/latest" \
            | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
        [ -n "$version" ] || die "could not determine the latest release from the forge"
    fi
    base="$FORGE_REPO/releases/download/$version"
elif [ -n "$version" ]; then
    base="$GITHUB_REPO/releases/download/$version"
else
    base="$GITHUB_REPO/releases/latest/download"
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
trap 'exit 1' INT TERM

say "Downloading $asset (${version:-latest}) from $source_name..."
fetch "$base/$asset" "$tmp/$asset" || die "download failed: $base/$asset"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || die "download failed: $base/SHA256SUMS"

expected=$(awk -v f="$asset" '{ n = $2; sub(/^\*/, "", n) } n == f { print $1 }' "$tmp/SHA256SUMS")
[ -n "$expected" ] || die "$asset not listed in SHA256SUMS"

if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$tmp/$asset" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$tmp/$asset" | awk '{ print $1 }')
else
    die "sha256sum or shasum is required to verify the download"
fi
[ "$expected" = "$actual" ] || die "checksum mismatch for $asset"

tar -xzf "$tmp/$asset" -C "$tmp" "$NAME" || die "archive does not contain $NAME"

mkdir -p "$bindir"
# Install via a temp file + mv so a running binary is replaced atomically.
cp "$tmp/$NAME" "$target.tmp"
chmod 755 "$target.tmp"
mv -f "$target.tmp" "$target"

say "Installed $("$target" --version 2>/dev/null || echo "$NAME") to $target"

case ":${PATH:-}:" in
    *":$bindir:"*) ;;
    *) warn "$bindir is not in your PATH. Add this to your shell profile:"
       printf '    export PATH="%s:%s"\n' "$bindir" "\$PATH" >&2 ;;
esac
