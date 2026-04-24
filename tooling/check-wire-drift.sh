#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-wire-drift.sh — Wire/local schema drift detection CI gate
#
# Runs the schema drift detection integration test that validates JSON Schema
# wire contracts match their corresponding Rust struct definitions.
#
# Exit codes:
#   0 — All registered schemas match their Rust types
#   1 — Drift detected or test failure
# ---------------------------------------------------------------------------
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Wire Schema Drift Detection ==="
echo "Checking that all registered schemas match their Rust struct definitions..."
echo ""

cd "$PROJECT_ROOT"

exec cargo test -p nexus-contracts --test schema_drift_detection "$@"
