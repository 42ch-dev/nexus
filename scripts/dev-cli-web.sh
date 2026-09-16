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
# 4. Runs the web dev server in the foreground (proxy → service via
#    VITE_DAEMON_URL).
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
health_ok() {
  node --input-type=module -e "
    const base = process.argv[1];
    try {
      const res = await fetch(base + '/v1/daemon/runtime/health');
      process.exit(res.ok ? 0 : 1);
    } catch { process.exit(1); }
  " "${BASE_URL}"
}

if health_ok; then
  echo "    service already running"
else
  echo "    starting standalone service (detached)"
  mkdir -p "${REPO_ROOT}/.dev"
  nohup node "${SERVICE_ROOT}/dist/main.js" --home "${HOME}" --host 127.0.0.1 --port "${PORT}" \
    > "${REPO_ROOT}/.dev/nexus-service.log" 2>&1 &
  SERVICE_PID=$!
  for _ in $(seq 1 50); do
    if health_ok; then break; fi
    sleep 0.2
  done
  health_ok || { echo "    service failed to become healthy; see .dev/nexus-service.log" >&2; exit 1; }
  echo "    service healthy (pid ${SERVICE_PID})"
fi

echo "==> validating running service compatibility"
node --input-type=module -e "
const base = process.argv[1];
const res = await fetch(base + '/v1/daemon/runtime/status');
const status = await res.json();
if (!res.ok) { console.error('service status failed'); process.exit(1); }
console.log('    standalone service compatible (runtime_mode ' + status.runtime_mode + ')');
" "${BASE_URL}"

echo "==> starting web dev server (http://localhost:5173)"
export VITE_DAEMON_URL
pnpm --filter web dev
