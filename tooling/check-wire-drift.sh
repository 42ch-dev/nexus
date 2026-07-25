#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-wire-drift.sh — Wire/local schema drift detection CI gate
#
# Two gates:
#   1. Spoke version conformance — the lockstep spoke pin (spoke-adapter-
#      architecture spec §1.1/§5.2) is honored in both the Rust workspace
#      Cargo.toml and the root npm package.json.
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
SPOKE_PIN="0.1.1"

echo "=== Spoke Version Conformance ==="
echo "Expected lockstep pin: ${SPOKE_PIN}"
echo ""

# ── Gate 1a: Rust crate pin (workspace Cargo.toml) ──────────────────────────
# The workspace [workspace.dependencies] declares `spoke-schemas = "=0.1.1"`.
CARGO_TOML="${PROJECT_ROOT}/Cargo.toml"
cargo_spoke_raw=$(grep -E '^[[:space:]]*spoke-schemas[[:space:]]*=' "$CARGO_TOML" | head -1)
# Strip to the version token inside the quotes, dropping the leading `=` (exact pin).
cargo_spoke=$(printf '%s' "$cargo_spoke_raw" | sed -E 's/.*"=?([^"]*)".*/\1/')

if [ -z "$cargo_spoke" ]; then
  echo "FAIL: spoke-schemas not found in ${CARGO_TOML}"
  exit 1
fi
if [ "$cargo_spoke" != "$SPOKE_PIN" ]; then
  echo "FAIL: Cargo.toml spoke-schemas='${cargo_spoke}' != pin '${SPOKE_PIN}'"
  exit 1
fi
echo "OK: Cargo.toml spoke-schemas = ${cargo_spoke}"

# ── Gate 1b: npm package pin (root package.json) ────────────────────────────
PKG_JSON="${PROJECT_ROOT}/package.json"
npm_spoke=$(node -e \
  "const d=require('${PKG_JSON}').dependencies||{}; const v=d['@42ch/spoke-schemas']; process.stdout.write(typeof v==='string'?v:'');" \
  2>/dev/null || true)

if [ -z "$npm_spoke" ]; then
  echo "FAIL: @42ch/spoke-schemas not found in ${PKG_JSON} dependencies"
  exit 1
fi
if [ "$npm_spoke" != "$SPOKE_PIN" ]; then
  echo "FAIL: package.json @42ch/spoke-schemas='${npm_spoke}' != pin '${SPOKE_PIN}'"
  exit 1
fi
echo "OK: package.json @42ch/spoke-schemas = ${npm_spoke}"
echo ""

echo "=== Wire Schema Drift Detection ==="
echo "Checking that all registered schemas match their Rust struct definitions..."
echo ""

cd "$PROJECT_ROOT"

exec cargo test -p nexus-contracts --test schema_drift_detection "$@"
