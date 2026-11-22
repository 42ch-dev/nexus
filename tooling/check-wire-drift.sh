#!/usr/bin/env bash
# note: check-wire-drift.sh — Wire/local schema drift detection CI gate
#
# Gates:
#   1. Spoke version conformance — the lockstep spoke pin (spoke-adapter-
#      architecture spec §1.1/§5.2) is honored in the Rust workspace
#      Cargo.toml (1a: the sibling crates match the canonical
#      `spoke-operations` entry) and the root npm package.json (1b), for
#      ALL spoke packages (spoke-schemas + spoke-operations + spoke-connect
#      crates; @42ch/spoke-schemas + @42ch/spoke-operations npm). All pins
#      must match.
#   2. Schema drift detection — the integration test that validates JSON Schema
#      wire contracts match their corresponding Rust struct definitions.
#
# Exit codes:
#   0 — All registered schemas match their Rust types AND spoke pins are honored
#   1 — Drift detected, test failure, or spoke pin mismatch
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# note: Lockstep spoke pin. The workspace Cargo.toml `spoke-operations` entry is the
# canonical source: every other declaration (the sibling crates, the root
# package.json) MUST match it. A dependency bump updates the manifest; this gate
# then catches a bump that missed a copy. `strategy-samples/**` is deliberately
# ungated: it carries only `${PIN}` / `<semver>` placeholders, so there is no
# version literal to compare. Never hardcode the version here — that turns every
# minor/patch bump into a red CI run and forces a hand edit that merely restates
# the manifest.
CARGO_TOML="${PROJECT_ROOT}/Cargo.toml"

cargo_pin_of() {
  local crate="$1" raw
  raw=$(grep -E "^[[:space:]]*${crate}[[:space:]]*=" "$CARGO_TOML" | head -1 || true)
  printf '%s' "$raw" | sed -E 's/.*"=?([^"]*)".*/\1/'
}

SPOKE_PIN="$(cargo_pin_of spoke-operations)"
if [ -z "$SPOKE_PIN" ]; then
  echo "FAIL: canonical spoke-operations pin not found in ${CARGO_TOML}"
  exit 1
fi

echo "=== Spoke Version Conformance ==="
echo "Expected lockstep pin: ${SPOKE_PIN}"
echo ""

# note: Gate 1a: Rust crate pins (workspace Cargo.toml)
# The workspace [workspace.dependencies] declares exact pins for all three
# crates; `spoke-operations` is the canonical source, so the other two are
# asserted against it:
#   spoke-schemas    = "<pin>"
#   spoke-operations = "<pin>"     (canonical)
#   spoke-connect    = "<pin>"     (opt-in behind feature `connect-host`)
for crate in spoke-schemas spoke-connect; do
  cargo_spoke=$(cargo_pin_of "$crate")

  if [ -z "$cargo_spoke" ]; then
    echo "FAIL: ${crate} not found in ${CARGO_TOML}"
    exit 1
  fi
  if [ "$cargo_spoke" != "$SPOKE_PIN" ]; then
    echo "FAIL: Cargo.toml ${crate}='${cargo_spoke}' != pin '${SPOKE_PIN}'"
    exit 1
  fi
  echo "OK: Cargo.toml ${crate} = ${cargo_spoke}"
done

# note: Gate 1b: npm package pins (root package.json)
PKG_JSON="${PROJECT_ROOT}/package.json"
for pkg in @42ch/spoke-schemas @42ch/spoke-operations; do
  npm_spoke=$(node -e \
    "const d=require('${PKG_JSON}').dependencies||{}; const v=d['${pkg}']; process.stdout.write(typeof v==='string'?v:'');" \
    2>/dev/null || true)

  if [ -z "$npm_spoke" ]; then
    echo "FAIL: ${pkg} not found in ${PKG_JSON} dependencies"
    exit 1
  fi
  if [ "$npm_spoke" != "$SPOKE_PIN" ]; then
    echo "FAIL: package.json ${pkg}='${npm_spoke}' != pin '${SPOKE_PIN}'"
    exit 1
  fi
  echo "OK: package.json ${pkg} = ${npm_spoke}"
done

echo "=== Wire Schema Drift Detection ==="
echo "Checking that all registered schemas match their Rust struct definitions..."
echo ""

cd "$PROJECT_ROOT"

exec cargo test -p nexus-contracts --test schema_drift_detection "$@"
