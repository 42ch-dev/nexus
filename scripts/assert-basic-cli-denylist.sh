#!/usr/bin/env bash
# Assert the basic-cli nexus42 dependency tree excludes §2.1 denylist crates.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

TREE_FILE="${1:-}"

if [[ -z "${TREE_FILE}" ]]; then
  TREE_FILE="$(mktemp)"
  # `cargo tree` takes a package spec only — no --bin/--target-dir
  # (CARGO_TARGET_DIR is honored via the environment).
  cargo tree -p nexus42 \
    --no-default-features --features basic-cli \
    --edges normal,build >"${TREE_FILE}"
fi

DENYLIST=(
  nexus-daemon-runtime
  nexus-orchestration
  nexus-agent-host
  nexus-acp-host
  nexus-wasm-host
  wasmtime
  graph-flow
  axum
  napi
  spoke-connect
  libp2p
  rmcp
)

violations=()
for crate in "${DENYLIST[@]}"; do
  if grep -q "${crate}" "${TREE_FILE}"; then
    violations+=("${crate}")
  fi
done

if ((${#violations[@]} > 0)); then
  echo "FAIL: basic-cli tree contains denylisted crates: ${violations[*]}" >&2
  echo "--- tree ---" >&2
  cat "${TREE_FILE}" >&2
  exit 1
fi

echo "PASS: basic-cli tree excludes all ${#DENYLIST[@]} §2.1 denylist crates"
echo "tree_file=${TREE_FILE}"
