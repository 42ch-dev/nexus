#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-wire-drift.sh — Wire/local schema drift detection CI gate
#
# Two gates:
#   1. Spoke version conformance — the lockstep spoke pin (spoke-adapter-
#      architecture spec §1.1/§5.2) is honored in both the Rust workspace
#      Cargo.toml and the root npm package.json, for BOTH spoke packages
#      (spoke-schemas + spoke-operations). All four pins must match.
#   2. Schema drift detection — the integration test that validates JSON Schema
#      wire contracts match their corresponding Rust struct definitions.
#
# Exit codes:
#   0 — All registered schemas match their Rust types AND spoke pins are honored
#   1 — Drift detected, test failure, or spoke pin mismatch
# ---------------------------------------------------------------------------
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Lockstep spoke pin (spoke-adapter-architecture spec §1.1). Bump this in
# lockstep across Cargo.toml + package.json when adopting a new spoke release.
SPOKE_PIN="0.6.0"

echo "=== Spoke Version Conformance ==="
echo "Expected lockstep pin: ${SPOKE_PIN}"
echo ""

# ── Gate 1a: Rust crate pins (workspace Cargo.toml) ─────────────────────────
# The workspace [workspace.dependencies] declares exact pins for both crates:
#   spoke-schemas    = "=0.6.0"
#   spoke-operations = "=0.6.0"
CARGO_TOML="${PROJECT_ROOT}/Cargo.toml"
for crate in spoke-schemas spoke-operations; do
  cargo_spoke_raw=$(grep -E "^[[:space:]]*${crate}[[:space:]]*=" "$CARGO_TOML" | head -1)
  # Strip to the version token inside the quotes, dropping the leading `=` (exact pin).
  cargo_spoke=$(printf '%s' "$cargo_spoke_raw" | sed -E 's/.*"=?([^"]*)".*/\1/')

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

# ── Gate 1b: npm package pins (root package.json) ───────────────────────────
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
echo ""

echo "=== Wire Schema Drift Detection ==="
echo "Checking that all registered schemas match their Rust struct definitions..."
echo ""

cd "$PROJECT_ROOT"

exec cargo test -p nexus-contracts --test schema_drift_detection "$@"
