#!/usr/bin/env bash
# One-command daemon-free preset validation for a strategy bundle.
#
# Usage:
#   ./validate.sh [STRATEGY_DIR]
#
# Defaults to the bundled game-narrative sample. Runs the REAL validator
# core in-process via `nexus42 system preset validate --offline` — no
# daemon needed (the `nexus-runtime` artifact does not serve the daemon
# HTTP router, so the partner can validate on any machine with the CLI).
#
# Exit status: 0 when the strategy validates clean, non-zero otherwise.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET="${1:-$SCRIPT_DIR/game-narrative}"

if ! command -v nexus42 >/dev/null 2>&1; then
    echo "error: 'nexus42' CLI not found on PATH" >&2
    echo "  build it once from the repo root: cargo build --bin nexus42" >&2
    exit 127
fi

echo "==> Validating strategy: $TARGET"
nexus42 system preset validate --offline "$TARGET"
