#!/usr/bin/env bash
# Local dev shortcut: compatible backend artifact reuse + Web dev server.
#
#   pnpm dev
#
# 1. Reuses a compatible nexus42 artifact when manifest/hash/protocol match.
# 2. Ensures the daemon is running on the selected loopback endpoint (starts detached if not).
# 3. Validates daemon health on that endpoint before starting Vite.
# 4. Runs the web dev server in the foreground (proxy → daemon via VITE_DAEMON_URL).
#
# Incompatible or missing artifacts fail fast with `pnpm dev:backend:refresh`.
# Stop: Ctrl-C kills the web dev server; the daemon stays up.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

if [ -z "${CARGO_TARGET_DIR:-}" ] && [ -d "${HOME}/.cache/nexus-target" ]; then
  export CARGO_TARGET_DIR="${HOME}/.cache/nexus-target"
fi

eval "$(node scripts/dev-backend-manifest.mjs --preflight)"
BIN="${NEXUS42_ARTIFACT}"
PORT="${NEXUS42_DAEMON_PORT}"

echo "==> backend artifact compatible (${BIN})"
echo "==> daemon endpoint ${VITE_DAEMON_URL}"

echo "==> ensuring daemon on ${VITE_DAEMON_URL}"
DAEMON_STATUS_OUTPUT="$("${BIN}" daemon status --port "${PORT}" 2>&1 || true)"
export DAEMON_STATUS_OUTPUT
if node --input-type=module -e "import { isDaemonCliStatusRunning } from './scripts/dev-backend-manifest.mjs'; process.exit(isDaemonCliStatusRunning(process.env.DAEMON_STATUS_OUTPUT ?? '') ? 0 : 1)"; then
  echo "    daemon already running"
else
  if "${BIN}" daemon start --port "${PORT}"; then
    echo "    daemon started (detached)"
  else
    echo "    daemon start issued; waiting for health confirmation"
  fi
fi

echo "==> validating running daemon compatibility"
node --input-type=module -e "import { readBackendManifest, assertCompatibleRunningDaemon } from './scripts/dev-backend-manifest.mjs'; const manifest = await readBackendManifest(process.env.NEXUS42_ARTIFACT); await assertCompatibleRunningDaemon({ baseUrl: process.env.VITE_DAEMON_URL, manifest, port: Number(process.env.NEXUS42_DAEMON_PORT), daemonStatusOutput: process.env.DAEMON_STATUS_OUTPUT ?? '' }); console.log('    running daemon compatible (version ' + manifest.packageVersion + ')');"

echo "==> starting web dev server (http://localhost:5173)"
export VITE_DAEMON_URL
pnpm --filter web dev
