#!/bin/sh
# Every head against the same input (expected.json): each encodes the same scene (checked by check.py), reads the
# same state, and saves a PNG through wgw-window. Heads whose language isn't installed are skipped, unless
# REQUIRE_ALL=1 (as in CI). Needs wgw-window: WGW_WINDOW, or target/release/wgw-window (cargo build --release
# -p wgw-window, after ./build.sh).
cd "$(dirname "$0")/../.."
root=$(pwd)
export WGW_WINDOW="${WGW_WINDOW:-$root/target/release/wgw-window}"
out=$(mktemp -d)
failed=0 skipped=""

head() {  # name, the command it needs, then how to run it
  name=$1 needs=$2; shift 2
  if ! command -v "$needs" >/dev/null; then skipped="$skipped $name"; return; fi
  if "$@" "$out" && python3 tests/heads/check.py "$out/$name.patch"; then echo "$name: ok"; else echo "$name: FAILED"; failed=1; fi
}

head python python3 python3 tests/heads/python.py
head julia julia julia tests/heads/julia.jl
head r Rscript Rscript tests/heads/r.R
head lean lake sh -c 'cd heads/lean && lake build -q wgw-test && .lake/build/bin/wgw-test "$0"'
head rust cargo cargo run -q -p wgw-show --example fixture --
rm -rf "$out"

if [ -n "$skipped" ]; then
  echo "skipped (not installed):$skipped"
  [ "${REQUIRE_ALL:-0}" = 1 ] && failed=1
fi
exit $failed
