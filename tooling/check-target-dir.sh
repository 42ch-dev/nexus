#!/usr/bin/env bash
# note: Assert this checkout builds into its own scoped CARGO_TARGET_DIR.
#
# `.envrc` decides the dir per checkout; a manual `export` in the session
# overrides it, and cargo reuses a fresh-looking `rmeta` another checkout
# produced (its fingerprint records the target dir and the package, never which
# checkout wrote the bytes). Nothing downstream notices until the build is
# wrong, so this check runs before a build rather than after a surprise.
#
# Run from the checkout root after `direnv allow` and after any manual export:
#   tooling/check-target-dir.sh
#
# Classification mirrors `.envrc` exactly: the main checkout owns the canonical
# `${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target`; every linked worktree
# (`.worktrees/<name>`) owns `nexus-target-<name>`.

set -euo pipefail

fail() {
  echo "::error::target dir: $1"
  exit 1
}

command -v git >/dev/null 2>&1 || fail "git not on PATH"
command -v cargo >/dev/null 2>&1 || fail "cargo not on PATH"
command -v jq >/dev/null 2>&1 || fail "jq not on PATH"

root=$(git rev-parse --show-toplevel 2>/dev/null) || fail "not inside a git checkout"

cache="${XDG_CACHE_HOME:-$HOME/.cache}"
if [[ -f "$root/.git" ]] && grep -qs 'worktrees' "$root/.git"; then
  expected="$cache/nexus-target-$(basename "$root")"
else
  expected="$cache/nexus-target"
fi

actual=$(cargo metadata --no-deps --format-version 1 --manifest-path "$root/Cargo.toml" \
  | jq -r .target_directory) || fail "cargo metadata failed for $root"

if [[ "$actual" != "$expected" ]]; then
  echo "::error::target dir: $(basename "$root") builds into $actual, this checkout owns $expected"
  echo "export CARGO_TARGET_DIR=$expected"
  exit 1
fi

echo "ok: $(basename "$root") builds into its own target dir ($actual)"
