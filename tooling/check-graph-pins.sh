#!/usr/bin/env bash
# V1.174 P0 T6 (AR-74) — dependency graph pins (CI, `schema-consistency-check`).
#
# Machine-checked, `--edges normal` only (dev-dependencies are excluded —
# they never ship in the distributed graph). Every probe asserts a COUNTER
# plus a VERSION match; nothing here is hand-inspected.
#
# Pins (spec §8 + AR-61 #2):
#   default (-p nexus-daemon-runtime / -p nexus42):
#     spoke-connect   ABSENT
#     libp2p          ABSENT
#     spoke-operations exactly one 0.11.1   (via nexus-spoke-adapter, prior art)
#     rmcp            exactly one 1.8.0     (via agent-client-protocol, prior art)
#   -F connect-client (both crates):
#     spoke-connect   exactly one 0.11.x
#     libp2p          exactly one 0.56.x    (spoke-connect base dep)
#     spoke-operations exactly one 0.11.1
#     rmcp            exactly one 1.8.0
#   -F connect-client,connect-host (nexus42 only; nexus-daemon-runtime has no
#     connect-host feature):
#     libp2p          exactly one 0.56.x
#     rmcp            exactly one 1.8.0
#
# DEV-DEP CAVEAT (AR-74): `cargo tree -p <crate>` includes dev-dependencies
# by default. `--edges normal` drops them — the pins below therefore verify
# the SHIPPED graph only. Tests-only harness deps (e.g. the rmcp CLIENT
# dev-dep in nexus42) are intentionally excluded here; the shipped graph
# keeps exactly one rmcp 1.8.0 (server + transport-io).
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
  out=$(cargo tree -p "$crate" $feats --edges normal -i "$pkg" 2>&1) || status=$?
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

# assert_exactly_one <crate> <features...> <package> <version-glob>
assert_exactly_one() {
  local crate="$1"; shift
  local feats="$1"; shift
  local pkg="$1"
  local want="$2"
  # QC-fix S-a: propagate the cargo-tree exit status (a failed invocation
  # must fail the pin loudly, not just yield an empty/one-line result).
  local out status=0
  # `|| status=$?` so a failing cargo tree does NOT trip `set -e` before
  # the status check (QC-fix S-a); `status` defaults to 0 for success.
  out=$(cargo tree -p "$crate" $feats --edges normal -i "$pkg" 2>&1) || status=$?
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
  local got="$versions"
  if [[ "$got" != $want ]]; then
    fail "$pkg for $crate $feats: expected version matching '$want', got '$got'"
  fi
  echo "ok: $pkg == $got ($want) for $crate $feats"
}

# --- default graph (no features) --------------------------------------------

for crate in nexus-daemon-runtime nexus42; do
  assert_empty "$crate" "" spoke-connect
  assert_empty "$crate" "" libp2p
  assert_exactly_one "$crate" "" spoke-operations "0.11.1"
  assert_exactly_one "$crate" "" rmcp "1.8.0"
done

# --- connect-client ------------------------------------------------------------
for crate in nexus-daemon-runtime nexus42; do
  assert_exactly_one "$crate" "--features connect-client" spoke-connect "0.11.*"
  assert_exactly_one "$crate" "--features connect-client" libp2p "0.56.*"
  assert_exactly_one "$crate" "--features connect-client" spoke-operations "0.11.1"
  assert_exactly_one "$crate" "--features connect-client" rmcp "1.8.0"
done

# --- connect-client + connect-host (nexus42 only) ------------------------------
assert_exactly_one nexus42 "--features connect-client,connect-host" libp2p "0.56.*"
assert_exactly_one nexus42 "--features connect-client,connect-host" rmcp "1.8.0"

echo "graph pins OK"
