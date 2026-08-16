---
module: cargo-dependencies
date: 2026-08-08
problem_type: tooling_decision
category: workflow-patterns
severity: medium
applies_when:
  - "Dependabot alerts on Rust crates that are never compiled (lockfile-only)"
  - "Deciding whether removing an unused cargo feature closes a security alert"
  - "Triaging dependabot rust alerts blocked on upstream crate versions"
tags: [cargo, dependabot, cargo-lock, feature-independent, hickory-proto, libp2p]
last_updated: 2026-08-16

# Cargo.lock is feature-independent — removing a feature does not remove its lockfile entries

## Context

V1.154 planned to close dependabot alerts #42/#43 (hickory-proto 0.25.2) by
removing the never-enabled `mdns` feature from `spoke-connect` (upstream). The
assumption: drop the feature → the package leaves the dependency graph → the
alerts close. **Empirically false.**

## Guidance

- **Cargo.lock is feature-independent**: it locks version entries for ALL
  optional dependencies of every crate in the graph, whether or not any
  feature enables them. Removing a feature removes the *activation*, not the
  lockfile entry.
- `cargo generate-lockfile` / `cargo update -p <pkg>` do **not** drop orphan
  optional-dep entries while the parent crate (e.g. libp2p 0.56.0) remains in
  the tree — verified 2026-08-07 with `cargo update -p libp2p-mdns` and
  `cargo generate-lockfile`.
- **Dependabot reports lockfile entries, not the activated graph** — the
  nexus repo was alerted on hickory-proto/yamux while the default build never
  compiled libp2p at all (feature-gated behind `connect-host`). Removing the
  feature does NOT close such alerts.
- The only real fixes: (a) the parent crate leaves the tree, or (b) upstream
  releases a version whose optional-dep spec moves off the vulnerable line
  (e.g. libp2p ≥0.57 for hickory-proto ≥0.26).

```bash
# Prove the activated graph is clean (useful evidence, but does NOT close alerts):
cargo tree -p spoke-connect -i hickory-proto        # "warning: nothing to print."
cargo tree -p spoke-connect -i libp2p-mdns          # "warning: nothing to print."
# The lockfile entries remain regardless:
grep -c 'name = "hickory-proto"' Cargo.lock         # 1 — still there
```

## Why This Matters

False assumptions here waste an entire cross-repo release cycle (spoke 0.9.2
was cut with the mdns removal) and then mislead the next triage. The removal
was still correct cleanup (dead code + activated graph + docs), but the
deferral target must name the upstream release, not the in-repo change.

## When to Apply

- Triaging any Rust dependabot alert where the package is optional/never
  compiled: first check whether the alert is lockfile-entry-based (it is, for
  optional deps), then find the actual unblock (upstream release or dep
  removal from the tree).
- Before planning "remove the feature to close the alert" for any crate whose
  parent stays in the tree.

## Examples

### Before
Plan: "spoke removes the `mdns` feature → hickory-proto leaves nexus's
Cargo.lock → alerts #42/#43 close."

### After
Reality: "spoke removes the `mdns` feature → activated graph clean, but
Cargo.lock keeps hickory-proto/libp2p-mdns/libp2p-dns entries while
libp2p 0.56.0 remains. Alerts close only with libp2p ≥0.57 (unreleased)."
Deferral recorded with the upstream target; the feature removal shipped as
cleanup, not as the alert fix.

## Update (V1.167, 2026-08-16): lockfile presence ≠ compile reachability — triage with feature-combo probes

Alert #41 (yamux 0.12.1, high) added a nuance the original doc did not
separate: a lockfile entry can be **never compiled under default features**
yet **activated under a feature combo CI actually builds**. In nexus,
`yamux@0.12.1` is absent from the default resolve but is an activated dep
(`libp2p 0.56.0 → libp2p-yamux 0.47.0 → {yamux 0.12.1, yamux 0.13.10}`) under
`--features nexus42/connect-host` — the combo `runtime-build.yml` /
`runtime-probe-build.yml` build for the `nexus-runtime` artifact. So #41 is a
**real reachable vulnerability in that artifact**, while #42/#43
(hickory-proto) remain lockfile-only false positives under every combo.

Triage recipe (all four probes, `SQLX_OFFLINE=true`, from repo root):

```bash
# 1. Default-feature resolve — is the pkg absent by default?
cargo metadata --format-version 1            # check resolve.nodes
# 2. Feature-on resolve (the combo CI builds) — does it appear?
cargo metadata --format-version 1 --features nexus42/connect-host
# 3. Compile truth for the vulnerable version:
cargo tree -p nexus42 --features connect-host --target all -i yamux@0.12.1
# 4. Compile truth for the other pkg:
cargo tree -p nexus42 --features connect-host --target all -i hickory-proto
```

Gotchas: `cargo metadata` has **no** `--target` flag (an invalid-flag run
silently proves nothing — V1.167 grill grounding was corrected for exactly
this); `cargo tree -i <pkg>@<version>` is the only probe that distinguishes
two co-resolved versions of the same crate. Check which feature combos the CI
workflows actually build before calling an entry "unreachable".

Outcome pattern: disposition-with-evidence (probe transcripts in the
iteration package) + upstream-unblock deferral; hand-pruned entries are
re-added by the next resolve (verified again 2026-08-16: removing the
hickory-proto block → next `cargo metadata` re-writes it).
