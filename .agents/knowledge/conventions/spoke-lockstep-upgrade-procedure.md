---
module: nexus workspace (spoke-* crate + npm pins, libp2p lockstep)
date: 2026-08-19
problem_type: convention
category: conventions
severity: medium
source_iteration: V1.169
created: 2026-08-19
last_updated: 2026-11-09
status: active
tags:
  - spoke
  - lockstep
  - dependency-pins
  - libp2p
  - drift-gate
  - lockfile
  - upgrade-procedure
applies_when:
  - '"Bumping any spoke-* pin (Cargo + npm + drift-gate + docs pins must move together)"'
  - '"A spoke release moves libp2p or another dependency that nexus42''s feature-gated CLI shares types with"'
  - '"Planning or reviewing a lockstep upgrade round end to end"'
---

# Spoke Lockstep Upgrade Procedure

Distilled from the V1.139→V1.169 spoke upgrade series (most recently 0.10.0 → 0.11.1, V1.169 P0). The nexus repo pins `spoke-schemas` / `spoke-operations` / `spoke-connect` exact (`=x.y.z`) in workspace `Cargo.toml` and follows spoke upstream releases lockstep. Each upgrade has a recurring, non-obvious surface beyond "bump the pin".

## Context

spoke releases bundle Rust crates + npm packages + schemas + a drift gate that CI enforces. A pin bump that only touches `Cargo.toml` fails CI (`tooling/check-wire-drift.sh` requires all five pins — 3 crates + 2 npm packages — plus the `strategy-samples/**` docs pins (Gate 1c) to equal `SPOKE_PIN`) and misses breakage hidden behind cargo features and examples.

## Guidance

1. **Bump all five pins in lockstep**: 3 `Cargo.toml` workspace pins + 2 npm pins (`packages/nexus-contracts/package.json` + wherever `@42ch/spoke-*` resolves) + `SPOKE_PIN` in `tooling/check-wire-drift.sh`. Refresh `Cargo.lock` surgically: `cargo update -p spoke-schemas -p spoke-operations -p spoke-connect` — never a wholesale update; review the lock diff for unrelated churn.
2. **Keep `strategy-samples/**` docs pins lockstep (Gate 1c, V1.170)**: every `@42ch/spoke-connect@<version>` occurrence in the integrator docs tree (README + forkable game-narrative templates) must equal `SPOKE_PIN` — `check-wire-drift.sh` Gate 1c greps the whole `strategy-samples/` tree and fails on any stale pin. Integrators copy the template bundles, so a stale pin there is the same rot channel the gate exists to close; sweep the tree with the same grep before bumping.
3. **Refresh `pnpm-lock.yaml`** via the repo's lockfile flow (V1.164 precedent). Check the workspace `minimumReleaseAge` policy first: the 720-minute window (pnpm 11 default) affects CI as much as a local machine — `pnpm install --frozen-lockfile` fails at the setup step and pnpm 11's preflight makes every `pnpm run <script>` fail too, until `publish time + 12h` — but `pnpm-workspace.yaml` may explicitly set `minimumReleaseAge: 0`, in which case a fresh publish installs immediately (0.16.0 round, 2026-11-09). Never edit the policy to dodge the wait; when the window does apply, plan the delivery sequence around that cutoff (no PR, no CI-green claim before it). See [../conventions/pnpm-toolchain-pin-and-supply-chain-age.md](pnpm-toolchain-pin-and-supply-chain-age.md).
4. **Compile BOTH graphs with `--all-targets`**: `cargo check --workspace --all-targets` and `--workspace --all-targets --features nexus42/connect-host`. Feature-gated **examples** only compile under `--all-targets` — the V1.169 bump surfaced a 4th `HostCapabilityManifest` struct-literal site in an example that the 3 known sites list missed.
5. **Upstream additive struct fields break struct literals** — expect them at every literal-construction site (see also [../build-errors/codegen-optional-field-callsite-coverage.md](../build-errors/codegen-optional-field-callsite-coverage.md)). Honest-empty declarations stay honest: `tools: Vec::new()` + upstream serde `skip_serializing_if` keeps the wire member omitted.
6. **Pin the refusal/honesty contract with tests when upstream adds a capability surface**: new op families (e.g. `tools.*` dispatch-gate prefix rule in 0.11.0) get (a) handler-level refusal tests (the SERVED_OPS gate precedes lane acquisition — probe it), (b) session-level refusal-matrix rows, (c) manifest assertions (no fabricated capability, wire-omitted members).
7. **Move a shared transport dependency in lockstep with the crate that exposes it.** The V1.195 round (0.13.1 → 0.14.1) had to move `libp2p` `=0.56.0` → `=0.57.0` because `spoke-connect` 0.14.1 requires that pin and `apps/nexus42`'s feature-gated `connect-host` CLI passes its **own** libp2p values (`PeerId`, `Multiaddr`) into `spoke-connect`'s public API — two coexisting libp2p versions cannot share those types. Two rules follow:
   - an upstream release that is schema-neutral and version-only in its operations crate is still a **multi-pin** change; read the upstream diff per crate (`git diff --stat <prev>..<new> -- crates/spoke-schemas crates/spoke-operations`) before calling it a re-pin;
   - re-run the whole graph-pin probe matrix afterwards: the default/domain graph must stay libp2p-free, the feature-on graph must resolve exactly one libp2p version, and no probe asserts a version *value* ([graph-pin-honesty-discipline.md](graph-pin-honesty-discipline.md)).
   Because `libp2p` is a transitive requirement of `spoke-connect` rather than a freely chosen dependency, treat a mismatch as an upstream compatibility fact, not a local preference.
8. **Feature-graph evidence set**: default graph libp2p-free (`cargo tree -p nexus42 -i libp2p` → absent), single libp2p version feature-on, single `regress` version both graphs.
9. **Record the trail** in the `Cargo.toml` pin comment block (per-iteration section, upstream change summary, and the reason for every lockstep companion pin — including the shared-type argument for `libp2p`) and align `docs/specs/architecture/spoke-adapter-architecture.md` §1.1/§5.2 pins in the same commit.
10. **Keep the parity proof cheap and permanent**: the adapter's own parity test (`cargo test -p nexus-spoke-adapter --test spoke_parity`) plus the two `cargo check` shapes are the round's behavioural evidence; a dependency-only round changes no product surface, so it must not grow product tests to look substantial.

## Why This Matters

The npm/drift-gate lockstep and the `--all-targets` example compile are the two steps most easily missed; both fail late (CI / feature-on build) and cost a full round-trip. The lockstep companion (`libp2p`) is the third: nothing in the pin diff itself says "move libp2p", so the requirement is only visible in `spoke-connect`'s own manifest and in the type-sharing call path of the feature-gated CLI.

## When to Apply

Any spoke version bump in the nexus workspace (lockstep policy, V1.139+).

## Examples

- V1.169 (0.10.0 → 0.11.1): additive `manifest.tools` — 4 literal sites (1 production builder + 3 feature-gated examples), npm + drift-gate lockstep, `tools.*` refusal pinning.
- V1.195 (0.13.1 → 0.14.1): upstream diff was schema-neutral (no `spoke-schemas` API change; `spoke-operations` version-only), so the round was pins + lockfile only — two Rust cards (crate re-pin with `Cargo.lock` refresh, then npm re-pin) with `spoke_parity` and both `cargo check` shapes as the proof, no product-surface change, and `libp2p` moving `=0.56.0` → `=0.57.0` in the same window because `spoke-connect` 0.14.1 requires it and the `connect-host` CLI shares libp2p types with that public API. The freshly published npm pins landed inside the 720-minute `minimumReleaseAge` window, which made the delivery sequence (not the implementation) wait for the cutoff.
- 0.14.1 → 0.15.1 (memory-family re-pin, no adoption): upstream `CHANGELOG` listed Added-only, yet 0.15.0's schema regeneration (explicit `title`s on the shared `ExtensionMap`/`ErrorDetails` subschemas) renamed the five generated `*ExtensionsKey` types into per-module `ExtensionKey`s, made `Scope.extensions` optional, and moved `SourceAnchor` fields behind shared wrappers — an 86-identifier + 49-literal consumer sweep across three nexus crates. Both lock closures moved with it: `Cargo.lock` gained `serde_jcs` + `sha2` (spoke-operations digest helpers) and collapsed the duplicate `regress 0.10.5` into the single `0.12.0`, while `pnpm-lock.yaml` gained `canonicalize` 3.0.0 + `@noble/hashes` 2.4.0 through `@42ch/spoke-operations@0.15.1` (the JCS/sha2 helpers) — a first-party dep that materializes a previously optional peer also re-resolves unrelated peer suffixes in the lock diff, so read that churn as expected rather than as a bad merge. Lessons: (1) read the upstream **generated API diff** (`spoke-schemas` generated modules in the sibling checkout), never the CHANGELOG or `--stat`, before calling a round a pure re-pin; (2) the §6 refusal pinning for a new capability family must be pinned against the face the product actually uses — `capability_missing` exists only on spoke-connect's optional remote responder, while the node gate nexus configures answers every deny with `op_unsupported`; (3) a field that 0.14.1 omitted when the map was empty but 0.15.1 omits only when `None` must map empty → `None` at every conversion site (`"computable": {}` wire regression, caught by QC, now regression-pinned).
- 0.15.2 → 0.16.0 (cognition family, UNCONSUMED; presence-sensitive sweep): `libp2p` stayed `=0.57.1` (no companion move); the real breakage was presence-sensitivity — `Scope.entry_ids/entry_types/timeline_event_ids` and `MemoryRecordsCreateRequest.anchors/source_context_ids` became `Vec → Option<Vec>` (absent vs empty must stay distinguishable on the wire, and stored provenance must keep the published array shape instead of `null`), `MindDelta.path` froze into RFC 6901 pointers rooted under `/modules/mental/<field>` or `/modules/cognition/{records,world_facts}` (a bare `"attention"` fixture is now gate-rejected), and `spoke-operations` dropped six scope pub APIs nexus never used (grep-proven). Cognition (`CAPABILITY_SPOKE_COGNITION`, 13th mind-state key) stays declared-but-UNCONSUMED per the standing refusal precedent. `CapabilityTokenProof`/`CapabilityClaims` were byte-identical, so the local mirror gained a feature-gated mirror-vs-published equivalence test instead of a rewrite (I-000332). `pnpm-workspace.yaml` has `minimumReleaseAge: 0` here, so the fresh npm publish installed immediately; `canonicalize` 3.x → 5.1.0 rode in through `@42ch/spoke-operations@0.16.0`, and the npm packages now declare `engines.node >= 22`.
