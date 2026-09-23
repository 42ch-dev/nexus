#!/usr/bin/env bash
# Dependency graph pins for the FINAL v1.193 products (CI job
# `verify-graph-pins` and the `schema-consistency-check` step).
#
# Machine-checked, `--edges normal,build` only (dev-dependencies are excluded —
# they never ship in the distributed graph). Every probe asserts graph STRUCTURE
# (absence, single resolved version, resolved feature set) — never a version VALUE;
# nothing here is hand-inspected.
#
# v1.193 P2-T13 collapsed the app feature lattice to two real product
# selectors (`cli` default + independent `connect-host`) and deleted the
# `nexus-daemon-runtime` crate, `legacy-cli` / `basic-cli` / `web-embed` and
# the app-only `connect-client` / `embedded-mcp` Model A selectors. The pins
# below therefore target the products that exist: ordinary CLI, Connect-only
# runtime, optional CLI Connect, core domain, core MCP/peer library and the
# native/TS host. Cohorts: `.mstar/specs/rust-core-service-boundary.md` §4.2.
#
# Pins (--edges normal,build):
#   -p nexus42 (cli,default):
#     spoke-connect   ABSENT
#     libp2p          ABSENT
#     rmcp            ABSENT   (the Model A rmcp/ACP bridge left with the
#                               removed connect-client/embedded-mcp selectors)
#     spoke-operations exactly one version   (via nexus-spoke-adapter, prior art)
#     agent-client-protocol exactly one version  (via nexus-acp-host normal edge)
#     graph-flow      exactly one version, EMPTY feature set (no postgres/rig)
#   -p nexus42 --no-default-features --features connect-host (Connect-only
#   runtime; `connect-host` never implies `cli`):
#     spoke-connect   exactly one version
#     libp2p          exactly one version    (spoke-connect base dep)
#     spoke-operations exactly one version
#     rmcp            ABSENT
#     agent-client-protocol ABSENT  (no ACP/agent-host edge in this cohort)
#   -p nexus42 --features connect-host (optional CLI Connect):
#     libp2p          exactly one version
#     spoke-connect   exactly one version
#     spoke-operations exactly one version
#     agent-client-protocol exactly one version
#     rmcp            ABSENT
#   -p nexus-core --no-default-features (core domain):
#     spoke-connect / libp2p / rmcp / graph-flow / nexus-wasm-host /
#     nexus-agent-host / nexus-acp-host / nexus-orchestration  ABSENT
#     spoke-operations exactly one version
#   -p nexus-core --no-default-features --features connect-client (core
#   MCP/peer library; the shipped app no longer selects it):
#     rmcp            exactly one version
#     spoke-connect   exactly one version
#     libp2p          exactly one version
#     spoke-operations exactly one version
#   -p nexus-core --no-default-features --features embedded-mcp (Model B
#   in-process embedded server; implies connect-client):
#     same pins as connect-client
#   -p nexus-core-node (native/TS host): nexus-core resolves
#     feats=[default,execution,provider-host] — the node-owned selection gains
#     the hosted production factory's Host-plane half (v1.195 P0-T5). `compute`
#     stays a P2-T3 window; the default/domain and Connect cohorts below keep
#     nexus-agent-host absent.
#
# Feature evidence (resolved feature set on the inverted probe row):
#   graph-flow MUST resolve with an EMPTY feature set everywhere.
#   nexus-spoke-adapter MUST resolve feats=[compute,default] for the ordinary
#     CLI and feats=[compute] (explicit, no defaults) for Connect-only — the
#     Connect host keeps peer compute + shared WASM cache (AC3/D20).
#
# DEV-DEP CAVEAT (AR-74): `cargo tree -p <crate>` includes dev-dependencies
# by default. `--edges normal,build` drops them — the pins below therefore
# verify the SHIPPED graph only.
#
# Run from the repository root. Requires a Rust toolchain + `cargo`.

set -euo pipefail

CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target}"
export CARGO_TARGET_DIR

# --- helpers --------------------------------------------------------------

fail() {
  echo "::error::graph pin failed: $1"
  exit 1
}

# assert_empty <spec> <features> <package>
assert_empty() {
  local crate="$1"; shift
  local feats="$1"; shift
  local pkg="$1"
  # QC-fix S-a: capture the cargo-tree exit status. A FAILING `cargo tree`
  # (typo'd package/feature, crate not in the workspace) yields empty output
  # and a zero count — with `|| true` that false-greened the "absent" pin.
  local out status=0
  # `|| status=$?` (not a bare capture) so a failing cargo tree does NOT
  # trip `set -e` before we can inspect its status (QC-fix S-a); `status`
  # defaults to 0 so `set -u` stays satisfied on success.
  out=$(cargo tree -p "$crate" $feats --edges normal,build -i "$pkg" 2>&1) || status=$?
  local count
  # Only count `<pkg> v<ver>` rows (an empty `-i` report prints a bare
  # "package not found" line; tree output also carries parent crate rows
  # like `nexus-spoke-adapter v0.1.0` that must not be counted).
  count=$(grep -c "^$pkg v[0-9]" <<<"$out" || true)
  if [[ "$status" -ne 0 ]]; then
    # `cargo tree -i <absent>` exits 101 with "did not match any packages" —
    # that IS the normal absence signal. Any OTHER nonzero outcome (feature
    # typo, invalid invocation, crate not in workspace) is a real tool
    # failure and must fail the pin instead of false-greening the count.
    if grep -qF "package ID specification \`$pkg\` did not match any packages" <<<"$out"; then
      : # legitimate absence — the zero-count check below confirms it
    else
      fail "cargo tree for $crate $feats (probe $pkg) failed (exit $status): $out"
    fi
  fi
  if [[ "$count" -ne 0 ]]; then
    fail "$pkg must be MISSING from $crate$feats graph, found $count entry/entries: $out"
  fi
  echo "ok: $pkg absent from $crate $feats"
}

# assert_exactly_one <crate> <features...> <package>
# Asserts the package resolves to EXACTLY ONE version in the cohort (the
# anti-version-split guard). No version value is asserted: requirements live
# in the manifests (single source of truth) and dependency bumps must not
# have to edit this script.
assert_exactly_one() {
  local crate="$1"; shift
  local feats="$1"; shift
  local pkg="$1"
  # QC-fix S-a: propagate the cargo-tree exit status (a failed invocation
  # must fail the pin loudly, not just yield an empty/one-line result).
  local out status=0
  # `|| status=$?` so a failing cargo tree does NOT trip `set -e` before
  # the status check (QC-fix S-a); `status` defaults to 0 for success.
  out=$(cargo tree -p "$crate" $feats --edges normal,build -i "$pkg" 2>&1) || status=$?
  local versions
  versions=$(grep "^$pkg v[0-9]" <<<"$out" | sed -E 's/.* v([^ ]+).*/\1/' | sort -u || true)
  local count
  count=$(wc -l <<<"$versions" | tr -d ' ')
  if [[ "$status" -ne 0 ]]; then
    # Same absence tolerance as assert_empty: a package legitimately absent
    # from the resolved graph (feature off) exits 101 with the canonical
    # message. Exact-one pins only ever hit this when the feature gate that
    # pulls the package is off — treat as absent-passthrough only if the
    # message confirms it; anything else is a real failure.
    if grep -qF "package ID specification \`$pkg\` did not match any packages" <<<"$out"; then
      fail "$pkg expected in $crate$feats graph, but absent (feature gate off?)"
    fi
    fail "cargo tree for $crate $feats (probe $pkg) failed (status $status): $out"
  fi
  if [[ "$count" -ne 1 ]]; then
    fail "$pkg for $crate $feats: expected exactly one version, got $count ($versions)"
  fi
  echo "ok: $pkg single version ($versions) for $crate $feats"
}

# assert_features <crate> <features...> <package> <feature-list>
# Resolves the package's ONE version/feature pair in the cohort and compares
# the FEATURE half. Used for the selected-edge evidence the cohort table
# requires (graph-flow featureless; spoke-adapter explicit `compute`). No
# version value is asserted (see assert_exactly_one).
assert_features() {
  local crate="$1"; shift
  local feats="$1"; shift
  local pkg="$1"; shift
  local want_feats="$1"
  local out status=0
  out=$(cargo tree -p "$crate" $feats --edges normal,build -i "$pkg" -f "{p} feats=[{f}]" 2>&1) || status=$?
  if [[ "$status" -ne 0 ]]; then
    fail "feature probe for $pkg in $crate ${feats:-<default>} failed (exit $status): $out"
  fi
  local pairs
  # `{p}` prints `<name> v<version> [(<path>)]`; keep only the version + the
  # resolved feature list, deduplicated (the inverted tree repeats a package's
  # parents, never its own row).
  pairs=$(grep -oE "^$pkg v[^ ]+( \([^)]*\))? feats=\[[^]]*\]" <<<"$out" \
    | sed -E 's/^[^ ]+ v([^ ]+)( \([^)]*\))? (feats=\[[^]]*\])$/\1 \3/' | sort -u)
  local count
  count=$(sed '/^$/d' <<<"$pairs" | wc -l | tr -d ' ')
  if [[ "$count" -ne 1 ]]; then
    fail "$pkg for $crate ${feats:-<default>}: expected exactly one resolved version/feature pair, got $count ($(tr '\n' ';' <<<"$pairs"))"
  fi
  local got_ver="${pairs%% *}"
  local got_feats="${pairs#* }"
  if [[ "$got_feats" != "feats=[$want_feats]" ]]; then
    fail "$pkg for $crate ${feats:-<default>}: expected feats=[$want_feats], got '$got_feats'"
  fi
  echo "ok: $pkg v$got_ver $got_feats for $crate ${feats:-<default>}"
}

# --- ordinary CLI (cli,default) --------------------------------------------

assert_empty nexus42 "" spoke-connect
assert_empty nexus42 "" libp2p
assert_empty nexus42 "" rmcp
assert_exactly_one nexus42 "" spoke-operations
# ACP rides the unconditional nexus-acp-host normal edge of the local
# ACP spawning the ordinary CLI keeps. Exactly one stable-v1 core.
assert_exactly_one nexus42 "" agent-client-protocol
# graph-flow exactly once (chronology/cron/ops library edges), with NO
# default features (no `postgres`, no `rig`) — asserted below via feats=[].
assert_exactly_one nexus42 "" graph-flow

# --- Connect-only runtime (connect-host, no defaults) -----------------------

assert_exactly_one nexus42 "--no-default-features --features connect-host" spoke-connect
assert_exactly_one nexus42 "--no-default-features --features connect-host" libp2p
assert_exactly_one nexus42 "--no-default-features --features connect-host" spoke-operations
assert_empty nexus42 "--no-default-features --features connect-host" rmcp
assert_empty nexus42 "--no-default-features --features connect-host" agent-client-protocol

# --- optional CLI Connect (cli + connect-host) ------------------------------

assert_exactly_one nexus42 "--features connect-host" spoke-connect
assert_exactly_one nexus42 "--features connect-host" libp2p
assert_exactly_one nexus42 "--features connect-host" spoke-operations
assert_exactly_one nexus42 "--features connect-host" agent-client-protocol
assert_empty nexus42 "--features connect-host" rmcp

# --- core domain (no defaults) ----------------------------------------------

assert_empty nexus-core "--no-default-features" spoke-connect
assert_empty nexus-core "--no-default-features" libp2p
assert_empty nexus-core "--no-default-features" rmcp
assert_empty nexus-core "--no-default-features" graph-flow
assert_empty nexus-core "--no-default-features" nexus-wasm-host
assert_empty nexus-core "--no-default-features" nexus-agent-host
assert_empty nexus-core "--no-default-features" nexus-acp-host
assert_empty nexus-core "--no-default-features" nexus-orchestration
assert_exactly_one nexus-core "--no-default-features" spoke-operations

# --- core MCP/peer library (app-only Model A selectors are gone) -------------

for feats in "--no-default-features --features connect-client" "--no-default-features --features embedded-mcp"; do
  assert_exactly_one nexus-core "$feats" rmcp
  assert_exactly_one nexus-core "$feats" spoke-connect
  assert_exactly_one nexus-core "$feats" libp2p
  assert_exactly_one nexus-core "$feats" spoke-operations
done

# --- native/TS host (nexus-core-node) ---------------------------------------

# v1.195 P0-T5: the native cohort resolves the hosted production factory's
# both halves (`execution` + `provider-host`); `compute` arrives only with the
# P2-T3 window.
assert_features nexus-core-node "" nexus-core "default,execution,provider-host"
# The cohort edge is real, not a feature-flag claim: the Host plane the hosted
# factory composes resolves in the node cohort.
assert_exactly_one nexus-core-node "" nexus-agent-host

# --- selected-edge feature evidence -----------------------------------------

for crate in nexus-orchestration nexus42 nexus-core-node; do
  assert_features "$crate" "" graph-flow ""
done

assert_features nexus42 "" nexus-spoke-adapter "compute,default"
assert_features nexus42 "--no-default-features --features connect-host" nexus-spoke-adapter "compute"

echo "graph pins OK"
