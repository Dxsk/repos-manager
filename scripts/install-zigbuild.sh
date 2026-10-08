#!/usr/bin/env bash
# CI helper: install zig and cargo-zigbuild (prebuilt) into ~/.local/bin,
# used to cross-compile the static musl Linux binaries.
#
# Usage: scripts/install-zigbuild.sh   (prints the bin dir to add to PATH)

set -euo pipefail

ZIG_VERSION="${ZIG_VERSION:-0.13.0}"
ZIGBUILD_VERSION="${ZIGBUILD_VERSION:-v0.23.4}"

case "$(uname -m)" in
    x86_64|amd64) arch=x86_64; zigbuild_target=x86_64-unknown-linux-musl ;;
    aarch64|arm64) arch=aarch64; zigbuild_target=aarch64-unknown-linux-gnu ;;
    *) echo "unsupported host: $(uname -m)" >&2; exit 1 ;;
esac

dest="$HOME/.local/bin"
opt="$HOME/.local/opt"
mkdir -p "$dest" "$opt"

curl -fsSL --retry 3 "https://ziglang.org/download/${ZIG_VERSION}/zig-linux-${arch}-${ZIG_VERSION}.tar.xz" \
    | tar -xJ -C "$opt"
ln -sf "$opt/zig-linux-${arch}-${ZIG_VERSION}/zig" "$dest/zig"

curl -fsSL --retry 3 "https://github.com/rust-cross/cargo-zigbuild/releases/download/${ZIGBUILD_VERSION}/cargo-zigbuild-${zigbuild_target}.tar.xz" \
    | tar -xJ -C "$dest" --strip-components=1 --wildcards '*/cargo-zigbuild'

echo "$dest"
