#!/usr/bin/env bash
# Build the `nexus42` CLI binary for the macOS desktop bundle targets and copy
# it into `apps/desktop/src-tauri/binaries/` with the target-triple suffix that
# Tauri `bundle.externalBin` expects.
#
# Usage:
#   bash scripts/fetch-sidecar.sh                    # default: x86_64-apple-darwin
#   bash scripts/fetch-sidecar.sh <target>...        # explicit targets
#   SIDECAR_TARGETS="<target>..." bash scripts/fetch-sidecar.sh
#
# Called automatically by `beforeBuildCommand` before `tauri build`.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${REPO_ROOT}/apps/desktop/src-tauri/binaries"

# V1.66 ships an x86_64 macOS app only (native to the macos-13 CI runner;
# universal/aarch64-native builds are deferred to V1.67+). Pass targets as
# command-line args or via SIDECAR_TARGETS to override (e.g. local universal).
if [ $# -gt 0 ]; then
  TARGETS=("$@")
elif [ -n "${SIDECAR_TARGETS:-}" ]; then
  read -ra TARGETS <<<"${SIDECAR_TARGETS}"
else
  TARGETS=(
    x86_64-apple-darwin
  )
fi

mkdir -p "${DEST}"

export SQLX_OFFLINE=true

for target in "${TARGETS[@]}"; do
  echo "==> Building nexus42 for ${target}..."
  rustup target add "${target}" 2>/dev/null || true
  cargo build --release -p nexus42 --target "${target}"
  cp "${REPO_ROOT}/target/${target}/release/nexus42" "${DEST}/nexus42-${target}"
  chmod +x "${DEST}/nexus42-${target}"
  echo "    -> ${DEST}/nexus42-${target}"
done

echo "==> Sidecar binaries ready:"
ls -la "${DEST}"/nexus42-*
