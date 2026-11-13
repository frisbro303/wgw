#!/bin/sh
# Build wgw into dist/: the Rust core compiled to WebAssembly, the JS modules, and wgw.py.
# Usage: ./build.sh [folder to copy dist/ into, e.g. ../endless-quest/theme/wgw]
set -e
cd "$(dirname "$0")"
wasm-pack build --release --target web --out-dir dist/core --out-name wgw_core --no-typescript --no-pack
rm -f dist/core/.gitignore
cp js/*.js python/wgw.py dist/
if [ -n "$1" ]; then
  rm -rf "$1" && mkdir -p "$1" && cp -R dist/. "$1"
  echo "copied to $1"
fi
