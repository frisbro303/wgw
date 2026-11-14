#!/bin/sh
# Every head carries wgw's version, Cargo.toml's ([workspace.package]): set them together, by hand, when it changes.
# CI runs this, so one left behind fails there.
cd "$(dirname "$0")/.."
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"$/\1/p' Cargo.toml)
failed=0
check() {  # file, its version
  if [ "$2" != "$version" ]; then echo "$1 says $2, Cargo.toml $version"; failed=1; fi
}
check heads/python/pyproject.toml "$(sed -n '/^\[project\]/,/^\[/s/^version = "\(.*\)"$/\1/p' heads/python/pyproject.toml)"
check heads/julia/Project.toml "$(sed -n 's/^version = "\(.*\)"$/\1/p' heads/julia/Project.toml | head -n 1)"
check heads/r/DESCRIPTION "$(sed -n 's/^Version: //p' heads/r/DESCRIPTION)"
check heads/lean/lakefile.toml "$(sed -n '1,/^\[/s/^version = "\(.*\)"$/\1/p' heads/lean/lakefile.toml)"
[ $failed = 0 ] && echo "every head is at $version"
exit $failed
