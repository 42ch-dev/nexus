#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-wire-drift.sh — Wire/local schema drift detection CI gate
#
# Gates:
#   1. Spoke version conformance — the lockstep spoke pin (spoke-adapter-
#      architecture spec §1.1/§5.2) is honored in the Rust workspace
#      Cargo.toml (1a), the root npm package.json (1b), and the integrator
#      docs `strategy-samples/README.md` (1c, V1.170 P0 AR-13), for ALL spoke
#      packages (spoke-schemas + spoke-operations + spoke-connect crates;
#      @42ch/spoke-schemas + @42ch/spoke-operations npm; @42ch/spoke-connect
#      docs). All pins must match.
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
SPOKE_PIN="0.11.1"

echo "=== Spoke Version Conformance ==="
echo "Expected lockstep pin: ${SPOKE_PIN}"
echo ""

# ── Gate 1a: Rust crate pins (workspace Cargo.toml) ─────────────────────────
# The workspace [workspace.dependencies] declares exact pins for all three
# crates:
#   spoke-schemas    = "=0.11.1"
#   spoke-operations = "=0.11.1"
#   spoke-connect    = "=0.11.1"   (opt-in behind feature `connect-host`)
CARGO_TOML="${PROJECT_ROOT}/Cargo.toml"
for crate in spoke-schemas spoke-operations spoke-connect; do
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

# ── Gate 1c: docs-pin conformance (strategy-samples/ tree) ─────────────────
# Every `@42ch/spoke-connect@<version>` occurrence in the integrator docs
# must equal the lockstep pin — the doc cannot rot back to an older spoke
# release (V1.170 P0, AR-13). Scoped to the WHOLE sample tree (README +
# forkable game-narrative templates), not just the README: integrators copy
# the template bundles, so a stale pin there is the same rot channel AR-13
# exists to close (P0 QC fix wave, qc1 W-1).
bad_doc_pin=0
while IFS= read -r ver; do
  if [ "$ver" != "$SPOKE_PIN" ]; then
    echo "FAIL: strategy-samples/** pins @42ch/spoke-connect@${ver} != pin '${SPOKE_PIN}'"
    bad_doc_pin=1
  fi
done < <(grep -oRE '@42ch/spoke-connect@[0-9]+\.[0-9]+\.[0-9]+' "${PROJECT_ROOT}/strategy-samples" | sed -E 's/.*@//')

if [ "$bad_doc_pin" != "0" ]; then
  echo "FAIL: strategy-samples/** docs pin is not lockstep with SPOKE_PIN=${SPOKE_PIN}"
  exit 1
fi
echo "OK: strategy-samples/** @42ch/spoke-connect pins = ${SPOKE_PIN}"
echo ""

echo "=== Wire Schema Drift Detection ==="
echo "Checking that all registered schemas match their Rust struct definitions..."
echo ""

cd "$PROJECT_ROOT"

exec cargo test -p nexus-contracts --test schema_drift_detection "$@"
