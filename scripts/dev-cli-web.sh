#!/usr/bin/env bash
# Local dev shortcut: standalone TS service + Web dev server.
#
#   pnpm dev
#
# 1. Requires an already-built native artifact (packages/nexus-native-<platform>/
#    native/nexus_core_node.node). A missing/incompatible artifact fails fast
#    with `pnpm dev:backend:refresh` — ordinary startup never runs Cargo.
# 2. Ensures the standalone TS service (apps/nexus-service) is running on the
#    selected loopback endpoint (starts detached if not). No old-daemon
#    fallback exists.
# 3. Validates TS service health on that endpoint before starting Vite.
# 4. Prints the tokenized browser access URL from the service log.
# 5. Runs the web dev server in the foreground (proxy → service via
#    VITE_DAEMON_URL).
#
# LAN dev (plain HTTP, no TLS): NEXUS42_DEV_HOST=0.0.0.0 pnpm dev
#   The service binds the LAN and implies --allow-remote; a remote bind still
#   requires NEXUS42_DAEMON_API_KEY, and the browser gets in through the
#   printed /__bootstrap link (session cookie).
#
# Stop: Ctrl-C kills the web dev server; the service stays up.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

eval "$(node scripts/dev-backend-manifest.mjs --preflight)"
PORT="${NEXUS42_DAEMON_PORT}"
BASE_URL="${VITE_DAEMON_URL%/}"

# ── 1. Native artifact must already be built (zero Cargo on this path). ─────
NODE_ARCH="$(node -p "process.platform === 'darwin' ? 'darwin-' + process.arch : (process.platform === 'win32' ? 'win32-x64-msvc' : process.platform + '-' + (process.arch === 'x64' ? 'x64-gnu' : process.arch))")"
ARTIFACT="${REPO_ROOT}/packages/nexus-native-${NODE_ARCH}/native/nexus_core_node.node"
if [ ! -f "${ARTIFACT}" ]; then
  echo "!! native artifact missing at ${ARTIFACT}" >&2
  echo "   run: pnpm dev:backend:refresh" >&2
  exit 1
fi
echo "==> native artifact (${ARTIFACT})"

# ── 2. Standalone TS service on the loopback endpoint. ──────────────────────
echo "==> service endpoint ${BASE_URL}"
SERVICE_ROOT="${REPO_ROOT}/apps/nexus-service"
if [ ! -f "${SERVICE_ROOT}/dist/main.js" ]; then
  echo "    building service (tsc, no Cargo)"
  (cd "${SERVICE_ROOT}" && npx tsc -p tsconfig.json)
fi

SERVICE_PID=""
# Health must be the standalone TS service, not any old daemon: the
# unguarded daemon/status envelope carries the TS-only implementation_scope.
health_ok() {
  node --input-type=module -e "
    const base = process.argv[1];
    try {
      const res = await fetch(base + '/v1/daemon/daemon/status');
      if (!res.ok) process.exit(1);
      const status = await res.json();
      const scope = String(status.implementation_scope ?? '');
      process.exit(scope.startsWith('standalone-service') ? 0 : 1);
    } catch { process.exit(1); }
  " "${BASE_URL}"
}

SERVICE_STARTED=0
if health_ok; then
  echo "    service already running"
else
  SERVICE_STARTED=1
  echo "    starting standalone service (detached)"
  mkdir -p "${REPO_ROOT}/.dev"
  SERVICE_ARGS=(--home "${HOME}" --host "${NEXUS42_DEV_HOST:-127.0.0.1}" --port "${PORT}")
  if [ "${NEXUS42_DEV_ALLOW_REMOTE:-0}" = "1" ]; then
    SERVICE_ARGS+=(--allow-remote)
  fi
  nohup node "${SERVICE_ROOT}/dist/main.js" "${SERVICE_ARGS[@]}" \
    > "${REPO_ROOT}/.dev/nexus-service.log" 2>&1 &
  SERVICE_PID=$!
  for _ in $(seq 1 50); do
    if health_ok; then break; fi
    sleep 0.2
  done
  health_ok || { echo "    service failed to become healthy; see .dev/nexus-service.log" >&2; exit 1; }
  echo "    service healthy (pid ${SERVICE_PID})"
fi

# Browser access (access spec §3.1): the tokenized URL from the LATEST service
# boot in the log. Tokens are single-use, so an earlier run's line is dead and
# only the last match is useful. Dev serves the SPA through Vite, so the
# service never auto-opens a browser here; the link is surfaced to copy.
SERVICE_LOG="${REPO_ROOT}/.dev/nexus-service.log"
BOOTSTRAP_URL=""
if [ -f "${SERVICE_LOG}" ]; then
  BOOTSTRAP_URL="$(grep -o '\[nexus-service\] open: [^ ]*' "${SERVICE_LOG}" | tail -1 | sed 's/^\[nexus-service\] open: //' || true)"
fi
if [ "${SERVICE_STARTED}" = "0" ]; then
  echo "    service was already running; its one-time access link came from that run"
  echo "    restart the service for a fresh access link"
elif [ -n "${BOOTSTRAP_URL}" ]; then
  echo "==> browser access: ${BOOTSTRAP_URL}"
else
  echo "    no bootstrap URL in ${SERVICE_LOG}; see the service log"
fi

echo "==> validating running service identity"
node --input-type=module -e "
const base = process.argv[1];
const res = await fetch(base + '/v1/daemon/daemon/status');
if (!res.ok) { console.error('service status failed'); process.exit(1); }
const status = await res.json();
const scope = String(status.implementation_scope ?? '');
if (!scope.startsWith('standalone-service')) {
  console.error('refusing: the running endpoint is not the standalone TS service (' + scope + ')');
  process.exit(1);
}
console.log('    standalone TS service compatible (runtime identity confirmed)');
" "${BASE_URL}"

echo "==> starting web dev server (http://localhost:5173)"
export VITE_DAEMON_URL
pnpm --filter web dev
