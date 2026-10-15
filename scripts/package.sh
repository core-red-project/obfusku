#!/usr/bin/env bash
# Archives the release binaries for $TARGET into dist/, under the asset
# names install.sh and install.ps1 download.
set -euo pipefail

: "${TARGET:?set TARGET to a Rust target triple}"
case "$TARGET" in
  x86_64-unknown-linux-gnu) suffix=linux-x86_64; ext="" ;;
  aarch64-apple-darwin)     suffix=macos-aarch64; ext="" ;;
  x86_64-pc-windows-msvc)   suffix=windows-x86_64; ext=".exe" ;;
  *) echo "unsupported TARGET: $TARGET" >&2; exit 1 ;;
esac

cargo build --release --locked --target "$TARGET" -p obfusku-cli -p obfusku-lsp
mkdir -p dist
cp "target/$TARGET/release/obfusku$ext" "dist/obfusku-$suffix$ext"
cp "target/$TARGET/release/obfusku-lsp$ext" "dist/obfusku-lsp-$suffix$ext"
