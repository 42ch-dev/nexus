#!/usr/bin/env bash
# Served-UI smoke test — verifies the daemon binary can start and serve the
# embedded web SPA over HTTP. Runs against a throwaway home directory so it is
# safe to execute in CI and locally.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SMOKE_HOME="$(mktemp -d)"
CREATIVE_ROOT="${SMOKE_HOME}/Documents/nexus/smoke/smoke"

# Pick a free ephemeral port so a stale local daemon on the default port
# does not silently shadow this test. When NEXUS_DAEMON_PORT is set, use it
# exactly and fail fast if something else is already listening there.
if [ -n "${NEXUS_DAEMON_PORT:-}" ]; then
  PORT="${NEXUS_DAEMON_PORT}"
else
  PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("", 0)); print(s.getsockname()[1]); s.close()')
fi
BASE="http://127.0.0.1:${PORT}"
CREATOR_ID="smoke"
WORKSPACE_SLUG="smoke"

# Best-effort helpers for port ownership checks.
listener_pid() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    # The `|| true` keeps this helper from returning a non-zero status when
    # no process is listening on the port (with `set -o pipefail` active).
    lsof -ti tcp:"${port}" 2>/dev/null | head -n 1 || true
  fi
}

process_cmdline() {
  local pid="$1"
  if command -v ps >/dev/null 2>&1; then
    ps -p "${pid}" -o command= 2>/dev/null || true
  elif [ -r "/proc/${pid}/cmdline" ]; then
    tr '\0' ' ' < "/proc/${pid}/cmdline" || true
  fi
}

fail() {
  echo "Error: $*" >&2
  exit 1
}

# Determine the daemon binary path without building yet. An explicit override
# is useful for local verification against an already-built artifact.
NEXUS42="${NEXUS42:-${REPO_ROOT}/target/release/nexus42}"

# Fail fast if the chosen port is occupied by an unrelated process. Only a
# leftover Nexus smoke daemon (same binary, same port, --foreground) may be
# terminated; everything else is left alone and the script aborts.
preexisting_pid="$(listener_pid "${PORT}")"
if [ -n "${preexisting_pid:-}" ]; then
  preexisting_cmd="$(process_cmdline "${preexisting_pid}")"
  if [[ "${preexisting_cmd}" == *"${NEXUS42}"*daemon*start*--port*"${PORT}"*--foreground* ]]; then
    echo "Stopping stale smoke daemon (PID ${preexisting_pid}) on port ${PORT}..."
    kill "${preexisting_pid}" 2>/dev/null || true
    for _ in {1..20}; do
      if ! kill -0 "${preexisting_pid}" 2>/dev/null; then
        break
      fi
      sleep 0.2
    done
    kill -9 "${preexisting_pid}" 2>/dev/null || true
    if [ -n "$(listener_pid "${PORT}")" ]; then
      fail "Port ${PORT} is still occupied by stale smoke daemon PID ${preexisting_pid}."
    fi
  else
    fail "Port ${PORT} is already in use by PID ${preexisting_pid} (${preexisting_cmd}). Refusing to start the smoke daemon to avoid killing an unrelated process. Set NEXUS_DAEMON_PORT to a free port or stop the existing listener."
  fi
fi

cd "${REPO_ROOT}"

# The embedded SPA is read at cargo build time; ensure the dist exists.
if [ -z "${SKIP_WEB_BUILD:-}" ]; then
  echo "Building web SPA..."
  pnpm --filter web build
fi

# Build the nexus42 release binary so the test exercises the same artifact that
# embeds the SPA (static assets are release-only), unless an override path was
# provided and the binary already exists.
if [ ! -x "${NEXUS42}" ]; then
  echo "Building nexus42 (release)..."
  cargo build -p nexus42 --release --quiet
fi

# Use a throwaway HOME so the smoke test does not touch the developer's
# ~/.nexus42. All operational paths are derived from HOME.
export HOME="${SMOKE_HOME}"
NEXUS_HOME="${SMOKE_HOME}/.nexus42"

# Seed a minimal config + workspace so the daemon can resolve its state DB
# without going through the interactive `creator workspace init` flow.
echo "Seeding throwaway workspace..."
mkdir -p "${NEXUS_HOME}"
mkdir -p "${CREATIVE_ROOT}/.nexus42"
mkdir -p "${NEXUS_HOME}/creators/${CREATOR_ID}/workspaces/${WORKSPACE_SLUG}"

cat > "${NEXUS_HOME}/config.toml" <<EOF
active_creator_id = "${CREATOR_ID}"
workspace_path = "${CREATIVE_ROOT}"

[active_workspace_slug_by_creator]
${CREATOR_ID} = "${WORKSPACE_SLUG}"
EOF

cat > "${CREATIVE_ROOT}/.nexus42/workspace.json" <<EOF
{
  "name": "Smoke Test",
  "version": 1,
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "creator_id": "${CREATOR_ID}",
  "workspace_slug": "${WORKSPACE_SLUG}"
}
EOF

cat > "${NEXUS_HOME}/creators/${CREATOR_ID}/workspaces/${WORKSPACE_SLUG}/meta.json" <<EOF
{
  "schema_version": 1,
  "creator_id": "${CREATOR_ID}",
  "workspace_slug": "${WORKSPACE_SLUG}",
  "local_root": "${CREATIVE_ROOT}",
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

# Initialize the SQLite schema so the daemon can open its state DB.
"${NEXUS42}" system db status >/dev/null

# Start the daemon in the foreground and background it so the script owns the
# process and can clean it up reliably.
echo "Starting daemon on ${BASE}..."
"${NEXUS42}" daemon start --port "${PORT}" --foreground >"${SMOKE_HOME}/daemon.log" 2>&1 &
DAEMON_PID=$!

cleanup() {
  echo "Stopping daemon..."
  if kill "${DAEMON_PID}" 2>/dev/null; then
    # Give the daemon a moment to shut down gracefully, then forcefully.
    for _ in {1..10}; do
      if ! kill -0 "${DAEMON_PID}" 2>/dev/null; then
        break
      fi
      sleep 0.2
    done
    kill -9 "${DAEMON_PID}" 2>/dev/null || true
  fi
  wait "${DAEMON_PID}" 2>/dev/null || true
  rm -rf "${SMOKE_HOME}"
}
trap cleanup EXIT

# Wait for the HTTP server to be ready, but fail immediately if the daemon
# process dies before the health endpoint responds.
ready=false
for _ in {1..30}; do
  if curl -fsS "${BASE}/v1/local/runtime/health" >/dev/null 2>&1; then
    ready=true
    break
  fi
  if ! kill -0 "${DAEMON_PID}" 2>/dev/null; then
    echo "Daemon exited before the health endpoint became ready. Log tail:" >&2
    if [ -r "${SMOKE_HOME}/daemon.log" ]; then
      tail -n 30 "${SMOKE_HOME}/daemon.log" >&2 || true
    fi
    exit 1
  fi
  sleep 1
done
if [ "${ready}" != "true" ]; then
  echo "Daemon health endpoint did not become ready within 30 seconds. Log tail:" >&2
  if [ -r "${SMOKE_HOME}/daemon.log" ]; then
    tail -n 30 "${SMOKE_HOME}/daemon.log" >&2 || true
  fi
  exit 1
fi

echo "Checking Local API health..."
curl -fsS "${BASE}/v1/local/runtime/health" | grep -q '"status"'

echo "Checking served Web UI..."
# The root path serves the embedded SPA; look for the basic HTML skeleton.
curl -fsS "${BASE}/" | grep -q '<html'

echo "Served-UI smoke passed."
