#!/usr/bin/env bash
# Local dev shortcut: CLI (daemon) + Web dev server — desktop excluded (CI-only).
#
#   pnpm dev
#
# 1. Builds the nexus42 CLI (incremental; quiet).
# 2. Ensures the daemon is running on 127.0.0.1:8420 (starts it detached if not).
# 3. Runs the Vite dev server in the foreground (proxy → daemon).
#
# Stop: Ctrl-C kills the web dev server; the daemon stays up (it is
# independently managed via `nexus42 daemon start|stop`).

set -euo pipefail

# Repo convention: shared target dir outside the checkout (CI + local use
# CARGO_TARGET_DIR=~/.cache/nexus-target). If unset, prefer the shared cache
# dir over an in-repo target/ so cargo stays incremental across checkouts.
if [ -z "${CARGO_TARGET_DIR:-}" ] && [ -d "${HOME}/.cache/nexus-target" ]; then
  export CARGO_TARGET_DIR="${HOME}/.cache/nexus-target"
fi
BIN="${CARGO_TARGET_DIR:-target}/debug/nexus42"
PORT="${NEXUS42_DAEMON_PORT:-8420}"

echo "==> building nexus42 CLI (incremental)"
cargo build -p nexus42

echo "==> ensuring daemon on 127.0.0.1:${PORT}"
if ! "${BIN}" daemon status --port "${PORT}" >/dev/null 2>&1; then
  "${BIN}" daemon start --port "${PORT}"
  echo "    daemon started (detached)"
else
  echo "    daemon already running"
fi

echo "==> starting web dev server (http://localhost:5173)"
pnpm --filter web dev
