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

# ── §2.1 direct-dependency allowlist ─────────────────────────────────────────
# The denylist above only bounds the *closure*. This bounds what nexus42 itself
# declares in the basic cohort — the check that keeps `chrono` (and anything
# else legacy-only) from creeping back in as a direct dependency.
ALLOWLIST=(
  nexus-core
  nexus-contracts
  nexus-home-layout
  clap
  tokio
  serde
  serde_json
  serde_yaml
  toml
  dirs
  anyhow
  tracing
  tracing-subscriber
  thiserror
  url
)

DEPTH1_FILE="${2:-}"
if [[ -z "${DEPTH1_FILE}" ]]; then
  DEPTH1_FILE="$(mktemp)"
  cargo tree -p nexus42 \
    --no-default-features --features basic-cli \
    --edges normal,build --depth 1 >"${DEPTH1_FILE}"
fi

# Direct dependency names: lines two levels into the tree, minus version.
# (No `mapfile`: macOS ships bash 3.2.)
DIRECT=()
while IFS= read -r crate; do
  [[ -n "${crate}" ]] && DIRECT+=("${crate}")
done < <(
  grep -oE '^[├└]── [a-zA-Z0-9_-]+' "${DEPTH1_FILE}" \
    | awk '{print $2}' | sort -u
)

unexpected=()
for crate in "${DIRECT[@]}"; do
  found=0
  for allowed in "${ALLOWLIST[@]}"; do
    [[ "${crate}" == "${allowed}" ]] && { found=1; break; }
  done
  (( found )) || unexpected+=("${crate}")
done

if ((${#unexpected[@]} > 0)); then
  echo "FAIL: basic-cli declares direct dependencies outside the §2.1 allowlist: ${unexpected[*]}" >&2
  echo "--- direct deps ---" >&2
  printf '%s\n' "${DIRECT[@]}" >&2
  exit 1
fi

echo "PASS: basic-cli direct dependencies all within the §2.1 allowlist (${#DIRECT[@]} crates)"
echo "direct_deps_file=${DEPTH1_FILE}"
