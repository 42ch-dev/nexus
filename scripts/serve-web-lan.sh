#!/usr/bin/env bash
# LAN web service launcher — mints a random API key per boot and prints it;
# pass NEXUS42_DAEMON_API_KEY=<k> to reuse one.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -z "${NEXUS42_DAEMON_API_KEY:-}" ]; then
  export NEXUS42_DAEMON_API_KEY="$(openssl rand -hex 24)"
  echo "[serve-web-lan] NEXUS42_DAEMON_API_KEY=$NEXUS42_DAEMON_API_KEY" >&2
fi
exec node apps/nexus-service/dist/main.js \
  --home "$HOME/.nexus42" \
  --host 0.0.0.0 \
  --allow-remote \
  --web-dist "$PWD/apps/web/dist" "$@"
