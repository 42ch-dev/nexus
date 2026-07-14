#!/usr/bin/env bash
# Build the `nexus42` CLI binary for the macOS desktop bundle targets and copy
# it into `apps/desktop/src-tauri/binaries/` with the target-triple suffix that
# Tauri `bundle.externalBin` expects.
#
# Usage:
#   bash scripts/fetch-sidecar.sh                    # default: aarch64-apple-darwin (release)
#   bash scripts/fetch-sidecar.sh <target>...        # explicit targets
#   SIDECAR_TARGETS="<target>..." bash scripts/fetch-sidecar.sh
#   SIDECAR_PROFILE=debug bash scripts/fetch-sidecar.sh   # faster local desktop iteration
#
# Called automatically by `beforeBuildCommand` before `tauri build` (release)
# and by `pnpm dev:desktop` / `dev:desktop:web` (debug via sidecar:dev).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${REPO_ROOT}/apps/desktop/src-tauri/binaries"

# Default target follows the host macOS arch so `pnpm sidecar:dev` works on
# both Apple Silicon and Intel. Override with args or SIDECAR_TARGETS (e.g.
# universal / CI pinning). Non-Darwin hosts keep the historical aarch64 default
# used by release packaging docs.
if [ $# -gt 0 ]; then
  TARGETS=("$@")
elif [ -n "${SIDECAR_TARGETS:-}" ]; then
  read -ra TARGETS <<<"${SIDECAR_TARGETS}"
else
  case "$(uname -s):$(uname -m)" in
    Darwin:arm64) TARGETS=(aarch64-apple-darwin) ;;
    Darwin:x86_64) TARGETS=(x86_64-apple-darwin) ;;
    *) TARGETS=(aarch64-apple-darwin) ;;
  esac
fi

PROFILE="${SIDECAR_PROFILE:-release}"
case "${PROFILE}" in
  debug|release) ;;
  *)
    echo "SIDECAR_PROFILE must be 'debug' or 'release' (got: ${PROFILE})" >&2
    exit 1
    ;;
esac

mkdir -p "${DEST}"

export SQLX_OFFLINE=true
# Honor repo-root .envrc (CARGO_TARGET_DIR) when copying sidecar artifacts.
CARGO_TARGET="${CARGO_TARGET_DIR:-${REPO_ROOT}/target}"

for target in "${TARGETS[@]}"; do
  echo "==> Building nexus42 (${PROFILE}) for ${target}..."
  rustup target add "${target}" 2>/dev/null || true
  if [ "${PROFILE}" = "release" ]; then
    cargo build --release -p nexus42 --target "${target}"
  else
    cargo build -p nexus42 --target "${target}"
  fi
  cp "${CARGO_TARGET}/${target}/${PROFILE}/nexus42" "${DEST}/nexus42-${target}"
  chmod +x "${DEST}/nexus42-${target}"
  echo "    -> ${DEST}/nexus42-${target}"
done

echo "==> Sidecar binaries ready (${PROFILE}):"
ls -la "${DEST}"/nexus42-*
