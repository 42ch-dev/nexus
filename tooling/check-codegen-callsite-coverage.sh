#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-codegen-callsite-coverage.sh — typify struct-literal callsite gate
# (R-V1163P2QC1-002 / V1.163 P1 failure class).
#
# `pnpm run codegen` (typify) can add fields to generated Rust structs without
# updating struct-literal callsites in downstream crates. Rust E0063 only
# surfaces when those consumer crates compile — not when the contracts crate
# is regenerated. This is the post-codegen assertion from
# knowledge/engineering/codegen-optional-field-callsite-coverage.md:
#   SQLX_OFFLINE=true cargo check --workspace
#
# SQLX_OFFLINE is mandatory: bare `cargo check` fails sqlx macros without the
# committed `.sqlx/` offline metadata.
#
# Requires bash: package.json `codegen:verify` and CI invoke this script
# explicitly via `bash`; it is a CI/dev gate, not portable to non-bash
# environments (CI runs ubuntu-latest; macOS/Linux dev shells qualify).
#
# Usage (from repository root):
#   bash tooling/check-codegen-callsite-coverage.sh
# After regen:
#   pnpm run codegen:verify
# ---------------------------------------------------------------------------
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export SQLX_OFFLINE=true

echo "==> Codegen callsite coverage (SQLX_OFFLINE=true cargo check --workspace)"
# Default lib/bin graph only (no --all-targets): #[cfg(test)] struct literals
# are caught downstream by verify-sqlx-offline / rust-checks clippy
# `--all-targets` (CI budget; boundary accepted by the L2 task-4 review).
cargo check --workspace
echo "✅ Workspace compiles after codegen (no E0063-class missing-field breaks)"
