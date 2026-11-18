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
# native/TS host. Cohorts: `docs/specs/architecture/rust-core-service-boundary.md` §3.2.
#
# Pins (--edges normal,build):
#   -p nexus42 (cli,default):
#     spoke-connect   ABSENT
#     libp2p          ABSENT
#     rmcp            ABSENT   (the Model A rmcp/ACP bridge left with the
#                               removed connect-client/embedded-mcp selectors)
#     spoke-operations exactly one version   (via nexus-spoke-adapter, prior art)
#     agent-client-protocol exactly one version  (via nexus-acp-host normal
#                             edge), resolved feature set the EMPTY default
#                             (`feats=[default]`). SDK opt-in contract: the
#                             pin enables no opt-in capability feature, so its
#                             native transports stay out of the graph —
#                             async-process (SDK `process`) and blocking
#                             (SDK `stdio`) ABSENT
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
#   -p nexus-connect --no-default-features (the extracted Connect stack):
#     nexus-core / nexus-orchestration  ABSENT  — the edge runs
#     `nexus-core → nexus-connect` only; core's own assertions above carry the
#     forward (`nexus-connect` appears in their resolved feature sets).
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
#     feats=[compute,default,execution,nexus-connect,operation-id,provider-host]
#     — the node-owned selection gains the hosted production factory's
#     Host-plane half (v1.195 P0-T5) and the WASM compute runtime its factory
#     installs (v1.195 P2-T3); v1.244 adds the `nexus-connect` forward through
#     `execution`. The default/domain and Connect cohorts below keep
#     nexus-agent-host and nexus-wasm-host absent.
#
# Feature evidence (resolved feature set on the inverted probe row):
#   graph-flow MUST resolve with an EMPTY feature set everywhere.
#   nexus-spoke-adapter MUST resolve feats=[compute,default] for both cohorts —
#     the Connect host keeps peer compute + shared WASM cache (AC3/D20). Since
#     v1.207 P2 the Connect-only lane links `execution` (the peer-tools
#     registry the subscribe lane serves through), whose `nexus-orchestration`
#     edge turns spoke-adapter's `default` on; `default` == `compute` there, so
#     the exact-match assertion still guards against a future heavy default
#     feature appearing in the Connect-only graph.
#
# DEV-DEP CAVEAT (AR-74): `cargo tree -p <crate>` includes dev-dependencies
# by default. `--edges normal,build` drops them — the pins below therefore
# verify the SHIPPED graph only.
#
# Run from the repository root. Requires a Rust toolchain + `cargo`.
#
# Arguments (v1.249 R3):
#   (none)                        full pin matrix above — unchanged CI behavior
#   --cohort <name>               run ONE scoped cohort instead of the matrix.
#                                 `domain` = core with no default features.
#                                 `catalog` = core with its DEFAULT features:
#                                 the pure canonical host-tool catalog compiles
#                                 into that cohort, so it keeps the core-local
#                                 authority/storage edges and resolves no
#                                 engine/Host/WASM/compute/Connect plane.
#                                 `policy` = the private std-only compute
#                                 policy leaf with zero dependencies.
#                                 `capability` = the enabled Connect library
#                                 (capability + transport, no engine plane),
#                                 the default WASM-free capability crate, the
#                                 real WASM host under an explicit compute
#                                 selection, and the native factory's actual
#                                 dependency set.
#                                 `authoring` = the strategy-authoring surface
#                                 as a SELECTION: the CLI and native selectors
#                                 that carry it, the compute/execution
#                                 selections that must not, and the
#                                 checker-owned consumer in
#                                 `tooling/fixtures/authoring-cohorts` under
#                                 both of its selections (its own manifest, so
#                                 `--manifest-path` never substitutes for it).
#                                 Other plans add their cohorts as their real
#                                 source lands; an unknown name fails.
#   --manifest-path <Cargo.toml>  resolve every probe against that workspace.
#                                 Used by `check-graph-pins.test.mjs` to run
#                                 the same assertions over disposable fixtures.
#
# The scoped cohort is what makes the target boundary checkable while the
# source slices land: `domain` proves the core-local storage/memory authority
# still resolves (the applied run identity is execution-owned), and that no
# engine/Host/WASM/Connect plane does.

set -euo pipefail

CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target}"
export CARGO_TARGET_DIR

# --- helpers --------------------------------------------------------------

fail() {
  echo "::error::graph pin failed: $1"
  exit 1
}

# cargo_tree <args...>
# Single place every probe resolves its workspace, so `--manifest-path` reaches
# all of them (fixture workspaces) and the default run stays byte-identical.
cargo_tree() {
  # QC-fix round 3 (QC2-F003): the wrapper carries NO network policy.
  # Offline-ness is decided by the CALLER's environment, not by whether a
  # manifest path was passed: fixture runs set `CARGO_NET_OFFLINE=true` in
  # their spawn env (cargoEnv()), so path-only fixtures stay isolated, while
  # repository probes — implicit or an explicit `--manifest-path` — resolve
  # online, where a cold cache can fetch registry metadata for crates
  # outside the probed tree.
  if [[ -n "${MANIFEST_PATH:-}" ]]; then
    cargo tree --manifest-path "$MANIFEST_PATH" "$@"
  else
    cargo tree "$@"
  fi
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
  out=$(cargo_tree -p "$crate" $feats --locked --edges normal,build -i "$pkg" 2>&1) || status=$?
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
  out=$(cargo_tree -p "$crate" $feats --locked --edges normal,build -i "$pkg" 2>&1) || status=$?
  local versions
  versions=$(grep "^$pkg v[0-9]" <<<"$out" | sed -E 's/.* v([^ ]+).*/\1/' | sort -u || true)
  local count
  # Count NONEMPTY rows: `wc -l` over an empty here-string counts its trailing
  # newline, so a successful query that matched no package reported one
  # "version" and passed the exact-one pin. An empty set is zero, never one.
  count=$(sed '/^$/d' <<<"$versions" | wc -l | tr -d ' ')
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
  out=$(cargo_tree -p "$crate" $feats --locked --edges normal,build -i "$pkg" -f "{p} feats=[{f}]" 2>&1) || status=$?
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

# assert_features_include <crate> <features...> <package> <feature>
# Asserts the package's ONE resolved feature list CONTAINS <feature>.
#
# The shipped selectors resolve long feature sets this guard does not own
# (`nexus42`'s `cli`, the native host's factory plane), so pinning their exact
# list again would duplicate the full matrix and break on any unrelated
# selection change. What the `authoring` cohort owns is one question per row:
# is the authoring surface selected? A containment probe answers exactly that,
# and it still fails loudly when the package resolves no row at all.
assert_features_include() {
  local crate="$1"; shift
  local feats="$1"; shift
  local pkg="$1"; shift
  local want="$1"
  local out status=0
  out=$(cargo_tree -p "$crate" $feats --locked --edges normal,build -i "$pkg" -f "{p} feats=[{f}]" 2>&1) || status=$?
  if [[ "$status" -ne 0 ]]; then
    fail "feature probe for $pkg in $crate ${feats:-<default>} failed (exit $status): $out"
  fi
  local pairs
  pairs=$(grep -oE "^$pkg v[^ ]+( \([^)]*\))? feats=\[[^]]*\]" <<<"$out" | sort -u)
  local count
  count=$(sed '/^$/d' <<<"$pairs" | wc -l | tr -d ' ')
  if [[ "$count" -ne 1 ]]; then
    fail "$pkg for $crate ${feats:-<default>}: expected exactly one resolved feature row, got $count ($(tr '\n' ';' <<<"$pairs"))"
  fi
  if ! grep -qE "feats=\[([^]]*,)?$want(,[^]]*)?\]" <<<"$pairs"; then
    fail "$pkg for $crate ${feats:-<default>}: expected feature '$want', got '$pairs'"
  fi
  echo "ok: $pkg selects feature $want for $crate ${feats:-<default>}"
}

# assert_graph_nonempty <crate> <features>
# Positive control for every absence pin: `cargo tree -i <pkg>` reports the
# same "did not match any packages" for a package that is absent from the
# resolved graph AND for a probe whose root resolved nothing, so a cohort with
# an empty graph would false-green the whole block. Require the resolved
# normal/build tree to name at least one package besides the root.
assert_graph_nonempty() {
  local crate="$1"; shift
  local feats="$1"
  local out err status=0
  # QC-fix round 2 (E): the temp file is removed on BOTH paths via an EXIT
  # trap — `fail` exits the script and would otherwise leak it.
  # QC-fix round 3 (QC1-09): the target dir is passed EXPLICITLY — BSD
  # mktemp ignores `TMPDIR`, so a bare `mktemp` never lands in the per-case
  # dir the fixture suite inspects and the cleanup assertion is vacuous.
  err=$(mktemp "${TMPDIR:-/tmp}/nexus-graph-pins-census.XXXXXXXX")
  trap 'rm -f "$err"' EXIT
  # QC-fix round 2 (B / QC2-F004): `--prefix none` makes the census
  # rendering-independent — no Unicode glyphs and no ASCII `|--` prefixes,
  # so line-anchored `name vX` rows are stable under any Cargo rendering.
  # stderr stays OUT of the census (QC2-F002); the exit status carries the
  # verdict.
  out=$(cargo_tree -p "$crate" $feats --locked --edges normal,build --prefix none 2>"$err") || status=$?
  if [[ "$status" -ne 0 ]]; then
    fail "graph probe for $crate ${feats:-<default>} failed (exit $status): $out$(cat "$err")"
  fi
  # QC2-F002: forward residual stderr on SUCCESS so legitimate warnings stay
  # visible without entering the census.
  if [[ -s "$err" ]]; then
    cat "$err" >&2
  fi
  rm -f "$err"
  trap - EXIT
  # Line-anchored rows of an explicitly un-prefixed tree: path text cannot
  # contribute a phantom package (QC2-F002), each package counted once.
  local packages
  packages=$(grep -E '^[A-Za-z0-9_.-]+ v[0-9]' <<<"$out" \
    | sed -E 's/^([A-Za-z0-9_.-]+) v[0-9].*/\1/' | sort -u)
  local rows
  rows=$(sed '/^$/d' <<<"$packages" | wc -l | tr -d ' ')
  if [[ "$rows" -lt 2 ]]; then
    fail "$crate ${feats:-<default>}: resolved graph names only $rows package(s); the absence pins would be vacuous: $out"
  fi
  echo "ok: $crate ${feats:-<default>} resolves $((rows - 1)) distinct dependency package(s)"
}

# assert_no_dependencies <crate> <features>
# Positive root AND leaf proof in one probe: the package must resolve (an
# absent or mistyped root fails the cargo tree invocation outright — no
# vacuous green), and its resolved normal/build tree must name ONLY the root
# itself — zero dependencies. Complements assert_graph_nonempty, which by
# design requires at least one dependency and so cannot pin a true leaf.
assert_no_dependencies() {
  local crate="$1"; shift
  local feats="$1"
  local out err status=0
  # QC-fix round 2 (E): EXIT trap so a failing census cleans up too.
  # QC-fix round 3 (QC1-09): explicit target dir — BSD mktemp ignores TMPDIR.
  err=$(mktemp "${TMPDIR:-/tmp}/nexus-graph-pins-census.XXXXXXXX")
  trap 'rm -f "$err"' EXIT
  # QC-fix round 2 (B / QC2-F004): `--prefix none` + line-anchored rows —
  # identical under Unicode and ASCII rendering, so a `|--`-prefixed ASCII
  # child row can never be dropped. stderr stays out of the census (QC2-F002).
  out=$(cargo_tree -p "$crate" $feats --locked --edges normal,build --prefix none 2>"$err") || status=$?
  if [[ "$status" -ne 0 ]]; then
    fail "graph probe for $crate ${feats:-<default>} failed (exit $status): $out$(cat "$err")"
  fi
  # QC2-F002: forward residual stderr on success, outside the census.
  if [[ -s "$err" ]]; then
    cat "$err" >&2
  fi
  rm -f "$err"
  trap - EXIT
  local packages
  packages=$(grep -E '^[A-Za-z0-9_.-]+ v[0-9]' <<<"$out" \
    | sed -E 's/^([A-Za-z0-9_.-]+) v[0-9].*/\1/' | sort -u)
  local rows
  rows=$(sed '/^$/d' <<<"$packages" | wc -l | tr -d ' ')
  if [[ "$rows" -ne 1 ]]; then
    fail "$crate ${feats:-<default>} must resolve with ZERO dependencies, got $rows package(s): $out"
  fi
  # QC-fix (QC2-F002): the sole row must BE the requested root — a census
  # that matched anything else resolved the wrong graph.
  if [[ "$packages" != "$crate" ]]; then
    fail "$crate ${feats:-<default>}: zero-dependency census matched '$packages', expected the root itself"
  fi
  echo "ok: $crate ${feats:-<default>} resolves with zero dependencies (root only)"
}

# --- cohort selection / manifest override -----------------------------------

COHORT=""
MANIFEST_PATH=""
# The checker-owned authoring consumer. Its rows always resolve against THIS
# manifest: the cohort's subject is the checked-in file, so a caller's
# `--manifest-path` cannot replace the consumer under test.
AUTHORING_CONSUMER_MANIFEST="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/fixtures/authoring-cohorts/Cargo.toml"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --cohort)
      [[ $# -ge 2 ]] || fail "--cohort requires a name"
      COHORT="$2"
      shift 2
      ;;
    --manifest-path)
      [[ $# -ge 2 ]] || fail "--manifest-path requires a Cargo.toml path"
      MANIFEST_PATH="$2"
      shift 2
      ;;
    -h|--help)
      echo "usage: $0 [--cohort domain|catalog|policy|capability|authoring] [--manifest-path <Cargo.toml>]"
      exit 0
      ;;
    *)
      fail "unknown argument '$1' (usage: $0 [--cohort domain|catalog|policy|capability|authoring] [--manifest-path <Cargo.toml>])"
      ;;
  esac
done
if [[ -n "$COHORT" ]]; then
  case "$COHORT" in
    domain | catalog | policy | capability | authoring) ;;
    *) fail "unknown cohort '$COHORT' (known: domain, catalog, policy, capability, authoring)" ;;
  esac
fi
if [[ -n "$MANIFEST_PATH" && ! -f "$MANIFEST_PATH" ]]; then
  fail "manifest not found: $MANIFEST_PATH"
fi

# --- scoped cohort: core domain (v1.249 R3) ---------------------------------
# The domain boundary every other cohort may grow from: `nexus-core` with no
# default features keeps its core-local storage/memory authority and resolves
# NOTHING from the engine, Host, WASM or Connect planes. The applied run
# identity is execution-owned (`nexus-session-storage`), so it is pinned absent
# with them. Cohort table:
# `docs/specs/architecture/rust-core-service-boundary.md` §3.2.
if [[ "$COHORT" == "domain" ]]; then
  assert_graph_nonempty nexus-core "--no-default-features"
  # `domain` selects no feature: core's own resolved set stays empty.
  assert_features nexus-core "--no-default-features" nexus-core ""
  assert_exactly_one nexus-core "--no-default-features" nexus-memory-provider
  assert_features nexus-core "--no-default-features" nexus-memory-provider "default"
  assert_exactly_one nexus-core "--no-default-features" spoke-operations
  # The provider's own forwarding edge is what carries `MemoryPort` into the
  # domain graph; pin it separately from core's direct spoke-operations edge.
  assert_exactly_one nexus-memory-provider "--no-default-features" spoke-operations

  # Engine + durable execution. The Simulate walk rides `nexus-orchestration`
  # (`nexus-orchestration/simulate`), so its absence is this pin.
  assert_empty nexus-core "--no-default-features" nexus-orchestration
  assert_empty nexus-core "--no-default-features" nexus-execution
  assert_empty nexus-core "--no-default-features" nexus-session-storage
  assert_empty nexus-core "--no-default-features" graph-flow
  # Host planes.
  assert_empty nexus-core "--no-default-features" nexus-agent-host
  assert_empty nexus-core "--no-default-features" nexus-acp-host
  assert_empty nexus-core "--no-default-features" nexus-wasm-host
  # Connect planes.
  assert_empty nexus-core "--no-default-features" nexus-connect
  assert_empty nexus-core "--no-default-features" spoke-connect
  assert_empty nexus-core "--no-default-features" libp2p
  assert_empty nexus-core "--no-default-features" rmcp

  echo "graph pins OK (cohort: domain)"
  exit 0
fi

# --- scoped cohort: private compute policy (v1.249 compute-policy P2) -------
# The private std-only policy leaf (v1.249): `nexus-compute-policy` resolves
# as the positive root with ZERO normal/build dependencies, so ANY dependency
# it gains — an engine, durable-execution, storage, Host, WASM or Connect
# back-edge — fails this cohort. The workspace-member public packages must not
# gain a private-policy edge either: the inverted probe keeps policy out of
# their resolved graphs. The three legit private consumers (`nexus-core`
# dev-only, `nexus-orchestration`, `nexus-wasm-host`) are intentionally NOT
# probed here — their edges are real and pinned by their own cohorts. The
# standalone non-workspace `nexus-module-{sdk,manifest,test}` crates cannot be
# resolved with `-p` from this workspace, so their isolation stays with the
# public-package readiness checks outside this guard.
if [[ "$COHORT" == "policy" ]]; then
  # QC-fix (v1.249 QC1-02 / QC2-F001): the leaf pin runs under EVERY target
  # and EVERY feature selection, not just the host target with default
  # features — a target- or feature-scoped dependency edge is invisible to
  # the host-default selectors (the feature-gated fixture in
  # check-graph-pins.test.mjs proves the widened selectors bite).
  # QC2-F005: the public inverse checks below run the ALL-target union
  # (`--target all`), which is conservative: a dependency that only exists on
  # a platform that never ships the public package would still be counted.
  assert_no_dependencies nexus-compute-policy "--all-features --target all"

  # The eleven former `assert_empty nexus-compute-policy …` back-edge probes
  # (nexus-orchestration, nexus-execution, nexus-session-storage, graph-flow,
  # nexus-agent-host, nexus-acp-host, nexus-wasm-host, nexus-connect,
  # spoke-connect, libp2p, rmcp) are DELETED: under a zero-dependency root
  # each could only ever return absent, and the zero-dependency leaf pin
  # above already refuses ANY dependency edge, including all eleven. The
  # separate public-to-policy inverse control below stays.
  for pkg in nexus-contracts nexus-workflow-ir nexus-pattern nexus-knowledge nexus-narrative; do
    assert_empty "$pkg" "--all-features --target all" nexus-compute-policy
  done

  echo "graph pins OK (cohort: policy)"
  exit 0
fi

# --- scoped cohort: canonical catalog (v1.249 S4) --------------------------
# The cohort the pure canonical host-tool catalog compiles into: `nexus-core`
# with its DEFAULT features. Its selection is empty today, so the shape matches
# `domain`; the cohort stays separate so the catalog keeps its own proof when
# the default selection grows. What it pins: the core-local authority/storage
# edges still resolve, `nexus-spoke-adapter` keeps resolving with NO features
# (the compute plane stays off), and no engine / Host / WASM / simulator /
# Connect plane appears.
if [[ "$COHORT" == "catalog" ]]; then
  assert_graph_nonempty nexus-core ""
  # Core's own resolved set stays the empty default.
  assert_features nexus-core "" nexus-core "default"
  # Positive authority + storage.
  assert_exactly_one nexus-core "" nexus-memory-provider
  assert_features nexus-core "" nexus-memory-provider "default"
  assert_exactly_one nexus-core "" spoke-operations
  # The adapter is a normal edge of the domain, but featureless: the compute
  # half (`compute` → the WASM host) is what this pin keeps out.
  assert_exactly_one nexus-core "" nexus-spoke-adapter
  assert_features nexus-core "" nexus-spoke-adapter ""

  # Engine + durable execution (the Simulate walk rides `nexus-orchestration`).
  assert_empty nexus-core "" nexus-orchestration
  assert_empty nexus-core "" nexus-execution
  assert_empty nexus-core "" nexus-session-storage
  assert_empty nexus-core "" graph-flow
  # Host planes.
  assert_empty nexus-core "" nexus-agent-host
  assert_empty nexus-core "" nexus-acp-host
  # Compute plane.
  assert_empty nexus-core "" nexus-wasm-host
  # Connect planes.
  assert_empty nexus-core "" nexus-connect
  assert_empty nexus-core "" spoke-connect
  assert_empty nexus-core "" libp2p
  assert_empty nexus-core "" rmcp

  echo "graph pins OK (cohort: catalog)"
  exit 0
fi

# --- scoped cohort: enabled capability graph (v1.249 C4) --------------------
# The capability extraction's integration proof, in four rows:
#
#  (1) The ENABLED Connect library. `connect-client,embedded-mcp` is the widest
#      feature selection `nexus-connect` ships, and it must resolve the two
#      planes the product keeps — the private capability crate (the registry
#      holder and its digest-poll watcher moved there in C3) and the WS
#      transport — while resolving NONE of the five planes the extraction cut:
#      core, orchestration, execution, session-storage and graph-flow. C3 moved
#      the capability tree out of orchestration, so the library edge onto
#      orchestration is obsolete rather than optional; this cohort is what
#      keeps it from coming back.
#  (2) The capability crate carries no WASM edge in EITHER selection: the
#      explicit `--no-default-features` build and the ordinary DEFAULT build
#      (the one `cargo build -p nexus-capability` produces) are separate rows,
#      because the claim "default excludes WASM" is about the default one.
#      `nexus-wasm-host` is an optional dependency bound only by the `wasm`
#      feature, so each absence pin is the visible proof of that gate.
#  (3) An EXPLICIT compute selection binds the REAL WASM host — one version, not
#      a stand-in — so "compute-off by default" cannot decay into "compute
#      gone".
#  (4) The NATIVE host keeps every dependency its hosted factory actually
#      installs: the capability crate, the WASM host behind core's `compute`
#      feature, the Agent Host, and core's exact resolved feature set.
#
# The Connect product staying engine-free is NOT asserted here: `nexus-core`'s
# own `connect-host` cohort still links `execution` by declaration and S6 owns
# that roster prerequisite cut. This cohort proves the library graph C4 owns.
if [[ "$COHORT" == "capability" ]]; then
  CONNECT_ENABLED="--no-default-features --features connect-client,embedded-mcp"
  assert_graph_nonempty nexus-connect "$CONNECT_ENABLED"
  assert_exactly_one nexus-connect "$CONNECT_ENABLED" nexus-capability
  assert_exactly_one nexus-connect "$CONNECT_ENABLED" spoke-connect
  assert_empty nexus-connect "$CONNECT_ENABLED" nexus-core
  assert_empty nexus-connect "$CONNECT_ENABLED" nexus-orchestration
  assert_empty nexus-connect "$CONNECT_ENABLED" nexus-execution
  assert_empty nexus-connect "$CONNECT_ENABLED" nexus-session-storage
  assert_empty nexus-connect "$CONNECT_ENABLED" graph-flow

  # Row (2), two SELECTIONS with two claims — never one row standing in for the
  # other (`--no-default-features` and the default selection coincide today only
  # because `nexus-capability` declares `default = []`; a future
  # `default = ["wasm"]` changes the ordinary build alone).
  #
  # Explicit `--no-default-features`: the bare crate carries no WASM edge.
  assert_graph_nonempty nexus-capability "--no-default-features"
  assert_empty nexus-capability "--no-default-features" nexus-wasm-host
  # DEFAULT selection (no flag): the build `cargo build -p nexus-capability`
  # produces carries no WASM edge. This is the row the claim "default excludes
  # WASM" belongs to; the fixture suite flips `default = ["wasm"]` to prove it
  # fails while the row above still passes.
  assert_graph_nonempty nexus-capability ""
  assert_empty nexus-capability "" nexus-wasm-host
  # Row (3): an EXPLICIT compute selection binds the REAL WASM host.
  assert_exactly_one nexus-capability "--features wasm" nexus-wasm-host

  assert_exactly_one nexus-core-node "" nexus-capability
  assert_exactly_one nexus-core-node "" nexus-wasm-host
  assert_exactly_one nexus-core-node "" nexus-agent-host
  assert_features nexus-core-node "" nexus-core "compute,default,execution,nexus-connect,operation-id,provider-host"

  echo "graph pins OK (cohort: capability)"
  exit 0
fi

# --- scoped cohort: authoring consumer (v1.249 A3) --------------------------
# One claim in three parts: strategy authoring is a SELECTED surface, never an
# implicit one.
#
# (1) The shipped selectors carry it. The ordinary CLI (`nexus42`, feature
#     `cli`) and the native host (`nexus-core-node`) each forward
#     `nexus-core/strategy-authoring`, so `cargo tree -i nexus-core` names the
#     feature and each selector resolves the private simulator
#     (`nexus-workflow-simulate`) exactly once. Containment rows, not
#     whole-set pins: the rest of each selector's feature list belongs to the
#     full matrix above and must not be restated here.
# (2) A selection without it stays clean. `nexus-core` with `compute` — the
#     widest non-authoring plane — and with no default features resolve NO
#     simulator, so the retired A1 intermediate ("compute links the
#     simulator") cannot come back silently.
# (3) The checker-owned consumer. `tooling/fixtures/authoring-cohorts` is its
#     own workspace with one path edge on `nexus-core` — the shape a real
#     caller has, not a core test target. Its DEFAULT is execution-only;
#     `--features authoring` adds core's authoring surface. Both resolved core
#     sets are pinned exactly, and the Host and WASM planes stay absent in
#     BOTH: a consumer gets the Host plane by selecting it, never by selecting
#     authoring.
if [[ "$COHORT" == "authoring" ]]; then
  # (1) Shipped authoring selectors.
  assert_exactly_one nexus42 "" nexus-workflow-simulate
  assert_features_include nexus42 "" nexus-core strategy-authoring
  assert_exactly_one nexus-core-node "" nexus-workflow-simulate
  assert_features_include nexus-core-node "" nexus-core strategy-authoring

  # (2) Non-authoring selections link no simulator.
  assert_empty nexus-core "--features compute" nexus-workflow-simulate
  assert_empty nexus-core "--no-default-features" nexus-workflow-simulate

  # (3) The consumer, against ITS OWN manifest — the checked-in file is the
  # subject here, so a caller's `--manifest-path` cannot substitute for it.
  MANIFEST_PATH="$AUTHORING_CONSUMER_MANIFEST"
  assert_graph_nonempty authoring-cohorts ""
  assert_features authoring-cohorts "" nexus-core "execution,nexus-connect,operation-id"
  assert_empty authoring-cohorts "" nexus-workflow-simulate
  assert_empty authoring-cohorts "" nexus-agent-host
  assert_empty authoring-cohorts "" nexus-wasm-host
  assert_exactly_one authoring-cohorts "--features authoring" nexus-workflow-simulate
  assert_features authoring-cohorts "--features authoring" nexus-core "execution,nexus-connect,operation-id,strategy-authoring"
  assert_empty authoring-cohorts "--features authoring" nexus-agent-host
  assert_empty authoring-cohorts "--features authoring" nexus-wasm-host

  echo "graph pins OK (cohort: authoring)"
  exit 0
fi

# --- ordinary CLI (cli,default) --------------------------------------------

assert_empty nexus42 "" spoke-connect
assert_empty nexus42 "" libp2p
assert_empty nexus42 "" rmcp
assert_exactly_one nexus42 "" spoke-operations
# ACP rides the unconditional nexus-acp-host normal edge of the local
# ACP spawning the ordinary CLI keeps. Exactly one stable-v1 core.
assert_exactly_one nexus42 "" agent-client-protocol
# Resolved set stays the empty default: the SDK declares `default = []` and
# the probe prints that default feature's name; any opt-in capability appends
# to the list and fails this probe — that is what the assertion is for.
assert_features nexus42 "" agent-client-protocol "default"
# v3 opt-in contract: the pin enables no opt-in capability feature, so the
# crates the SDK's `process` / `stdio` features would pull stay absent.
assert_empty nexus42 "" async-process
assert_empty nexus42 "" blocking
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
# v1.245: the run engine is an OPTIONAL edge enabled only by core's `execution`
# feature. This pin is what makes "absent from the default dependency graph"
# checkable instead of resting on the `optional = true` declaration alone.
assert_empty nexus-core "--no-default-features" nexus-execution
assert_exactly_one nexus-core "--no-default-features" spoke-operations
# The forward half of the same edge: with `execution` on, core resolves the
# extracted engine exactly once (no version split between the worked edge and
# the dev-time self-reference).
assert_exactly_one nexus-core "--features execution" nexus-execution

# --- core MCP/peer library + Connect-host product cohort --------------------
# v1.196 P1-T1: the cohort's public peer API (peer-tool types, capability and
# async-trait) is execution-owned, so `connect-client` implies `execution` and
# the resolved normal feature set is asserted exactly below. Graph structure
# only — never a version value.
#
# v1.244: `nexus-connect` is core's optional Connect edge. Core forwards to it
# as `nexus-connect/<feature>` and never says `dep:nexus-connect`, so Cargo
# keeps the implicit optional-dependency feature named `nexus-connect` and it
# appears in every closure that turns the edge on. That entry is asserted, not
# tolerated: it is the visible mark of the one edge the extraction added, and a
# `dep:` rewrite would keep the same dependency while hiding it from this pin.

# v1.207 P2: `connect-host` is the Connect-host product cohort — the WS accept
# loop + peer-tools event lane the shipped `nexus-runtime` boots, carrying NO
# rmcp (the reverse-invoke/operator bridge is not a Connect-host surface). It
# is what `apps/nexus42`'s `connect-host` feature links; the app must never
# select `connect-client` (which would pull rmcp into the product graph).
assert_empty nexus-core "--no-default-features --features connect-host" rmcp
assert_exactly_one nexus-core "--no-default-features --features connect-host" spoke-connect
assert_features nexus-core "--no-default-features --features connect-host" nexus-core "connect-host,execution,nexus-connect,operation-id"

for feats in "--no-default-features --features connect-client" "--no-default-features --features embedded-mcp"; do
  assert_exactly_one nexus-core "$feats" rmcp
  assert_exactly_one nexus-core "$feats" spoke-connect
  assert_exactly_one nexus-core "$feats" libp2p
  assert_exactly_one nexus-core "$feats" spoke-operations
done

# The cohorts resolve distinct feature sets (`connect-client` = `connect-host`
# + rmcp; `embedded-mcp` additionally implies `connect-client`), and
# `execution` implies `operation-id`, so each closure is asserted separately.
assert_features nexus-core "--no-default-features --features connect-client" nexus-core "connect-client,connect-host,execution,nexus-connect,operation-id"
assert_features nexus-core "--no-default-features --features embedded-mcp" nexus-core "connect-client,connect-host,embedded-mcp,execution,nexus-connect,operation-id"

# --- nexus-connect default graph -------------------------------------------
# v1.244: the extracted Connect stack must not depend back on the crate that
# depends on it. `--no-default-features` is the whole-crate fan-in proof: the
# unfeatured graph is three crates, so a core or orchestration edge reaching
# this pin means the direction reversed.
assert_empty nexus-connect "--no-default-features" nexus-core
assert_empty nexus-connect "--no-default-features" nexus-orchestration

# --- nexus-execution default graph -----------------------------------------
# v1.245: the extracted run engine must not depend back on the crate that
# composes it. `--no-default-features` is the whole-crate fan-in proof: the
# unfeatured graph names no core, so an edge reaching this pin means the
# extraction direction reversed.
assert_empty nexus-execution "--no-default-features" nexus-core

# --- nexus-memory-provider default graph -----------------------------------
# v1.245: the extracted in-process memory provider must not depend back on the
# crate that opens it, nor on the orchestration layer. The edge runs
# `nexus-core → nexus-memory-provider` only.
#
# The forward half is the opposite shape from `nexus-execution`'s: the provider
# is NOT optional (`service.rs` pins it on every core open, and `lib.rs`
# declared the module ungated before the move), so "absent from core's default
# graph" is false by construction here. What this pin asserts instead is that
# the default graph resolves it exactly once — no version split between the
# worked edge and the dev-time self-reference.
assert_empty nexus-memory-provider "--no-default-features" nexus-core
assert_empty nexus-memory-provider "--no-default-features" nexus-orchestration
assert_exactly_one nexus-core "--no-default-features" nexus-memory-provider

# --- native/TS host (nexus-core-node) ---------------------------------------

# v1.195 P0-T5: the native cohort resolves the hosted production factory's
# both halves (`execution` + `provider-host`); v1.195 P2-T3 adds the `compute`
# runtime that same factory installs (ONE WASM engine/cache/serializer).
# `execution` implies `operation-id` (v1.207 P3-T1), so it appears in the set;
# v1.244, `execution` is also the feature that turns on the optional
# `nexus-connect` edge, so its implicit feature name appears there too.
assert_features nexus-core-node "" nexus-core "compute,default,execution,nexus-connect,operation-id,provider-host"
# The cohort edge is real, not a feature-flag claim: the Host plane the hosted
# factory composes resolves in the node cohort.
assert_exactly_one nexus-core-node "" nexus-agent-host

# --- selected-edge feature evidence -----------------------------------------

for crate in nexus-orchestration nexus42 nexus-core-node; do
  assert_features "$crate" "" graph-flow ""
done

# V249 C3: the engine no longer carries a direct spoke-adapter edge, so the
# ordinary CLI cohort resolves the adapter FEATURELESS — the compute half
# (`default` = `compute` → nexus-wasm-host) that used to ride in through
# `nexus-orchestration` is gone. That is the accidental-WASM removal this plan
# owns: the CLI's own `cli` feature enables no compute route, so nothing it
# ships loses a surface, and the Connect cohort below states its compute edge
# explicitly instead of inheriting one.
assert_features nexus42 "" nexus-spoke-adapter ""
# Since v1.207 P2 the Connect-only lane boots nexus-core's peer-tools event
# lane. Since V249 C3 the compute half comes from the app's EXPLICIT
# `nexus-spoke-adapter/compute` feature entry (peer compute + shared WASM cache
# stay retained Connect behavior) rather than orchestration's `default`, so the
# resolved set is `compute` alone; the guard still asserts the exact set.
assert_features nexus42 "--no-default-features --features connect-host" nexus-spoke-adapter "compute"

echo "graph pins OK"
