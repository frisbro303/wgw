#!/bin/sh
# Build wgw into dist/: the Rust core compiled to WebAssembly, and the JS modules.
# Usage: ./build.sh [folder to copy dist/ into, e.g. ../endless-quest/theme/wgw]
#
# Needs the wasm32-unknown-unknown target, wasm-bindgen-cli at the version Cargo.lock has (it says how to install
# it if not), and, to make the core smaller, wasm-opt (binaryen).
set -e
cd "$(dirname "$0")"

# wgw's version: [workspace.package]'s in Cargo.toml, the one place it's written.
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
bindgen=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p')
if [ "$(wasm-bindgen --version 2>/dev/null)" != "wasm-bindgen $bindgen" ]; then
  echo "build.sh: needs wasm-bindgen $bindgen: cargo install wasm-bindgen-cli --version $bindgen --locked" >&2
  exit 1
fi

cargo build --release --target wasm32-unknown-unknown -p wgw
mkdir -p dist/core
wasm-bindgen --target web --no-typescript --out-dir dist/core --out-name wgw_core \
  target/wasm32-unknown-unknown/release/wgw.wasm
if command -v wasm-opt >/dev/null; then
  wasm-opt -Os dist/core/wgw_core_bg.wasm -o dist/core/wgw_core_bg.wasm
else
  echo "build.sh: no wasm-opt (binaryen), so the core isn't optimized for size" >&2
fi
# The heads (python/, julia/, r/, lean/) carry the same version.
sed -i.bak "s/^version = \".*\"$/version = \"$version\"/" python/pyproject.toml julia/Wgw/Project.toml lean/lakefile.toml
sed -i.bak "s/^Version: .*$/Version: $version/" r/wgw/DESCRIPTION
rm -f python/pyproject.toml.bak julia/Wgw/Project.toml.bak lean/lakefile.toml.bak r/wgw/DESCRIPTION.bak
cp js/*.js assets/logo.svg dist/
sed "s/__WGW_VERSION__/$version/" js/wgw.js > dist/wgw.js
echo "wgw $version: dist/ ($(($(wc -c < dist/core/wgw_core_bg.wasm) / 1024)) KB core)"

echo "for the Python, Julia, R, Lean and Rust heads: cargo build --release -p wgw-window (or cargo install --path window)"

if [ -n "$1" ]; then
  rm -rf "$1" && mkdir -p "$1" && cp -R dist/. "$1"
  echo "copied to $1"
fi
