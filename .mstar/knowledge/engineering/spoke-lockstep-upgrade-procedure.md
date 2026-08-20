---
title: Spoke lockstep upgrade procedure
category: engineering
track: knowledge
source_iteration: V1.169
created: 2026-08-19
last_updated: 2026-08-19
status: active
---

# Spoke Lockstep Upgrade Procedure

Distilled from the V1.139→V1.169 spoke upgrade series (most recently 0.10.0 → 0.11.1, V1.169 P0). The nexus repo pins `spoke-schemas` / `spoke-operations` / `spoke-connect` exact (`=x.y.z`) in workspace `Cargo.toml` and follows spoke upstream releases lockstep. Each upgrade has a recurring, non-obvious surface beyond "bump the pin".

## Context

spoke releases bundle Rust crates + npm packages + schemas + a drift gate that CI enforces. A pin bump that only touches `Cargo.toml` fails CI (`tooling/check-wire-drift.sh` requires all five pins — 3 crates + 2 npm packages — to equal `SPOKE_PIN`) and misses breakage hidden behind cargo features and examples.

## Guidance

1. **Bump all five pins in lockstep**: 3 `Cargo.toml` workspace pins + 2 npm pins (`packages/nexus-contracts/package.json` + wherever `@42ch/spoke-*` resolves) + `SPOKE_PIN` in `tooling/check-wire-drift.sh`. Refresh `Cargo.lock` surgically: `cargo update -p spoke-schemas -p spoke-operations -p spoke-connect` — never a wholesale update; review the lock diff for unrelated churn.
2. **Refresh `pnpm-lock.yaml`** via the repo's lockfile flow (V1.164 precedent). Note: a local user `~/.npmrc` `minimumReleaseAge` policy can block fresh packages on frozen install locally — CI is unaffected; hashes come from pnpm's own resolution.
3. **Compile BOTH graphs with `--all-targets`**: `cargo check --workspace --all-targets` and `--workspace --all-targets --features nexus42/connect-host`. Feature-gated **examples** only compile under `--all-targets` — the V1.169 bump surfaced a 4th `HostCapabilityManifest` struct-literal site in an example that the 3 known sites list missed.
4. **Upstream additive struct fields break struct literals** — expect them at every literal-construction site (see also [codegen-optional-field-callsite-coverage.md](codegen-optional-field-callsite-coverage.md)). Honest-empty declarations stay honest: `tools: Vec::new()` + upstream serde `skip_serializing_if` keeps the wire member omitted.
5. **Pin the refusal/honesty contract with tests when upstream adds a capability surface**: new op families (e.g. `tools.*` dispatch-gate prefix rule in 0.11.0) get (a) handler-level refusal tests (the SERVED_OPS gate precedes lane acquisition — probe it), (b) session-level refusal-matrix rows, (c) manifest assertions (no fabricated capability, wire-omitted members).
6. **Feature-graph evidence set**: default graph libp2p-free (`cargo tree -p nexus42 -i libp2p` → absent), single libp2p version feature-on, single `regress` version both graphs.
7. **Record the trail** in the `Cargo.toml` pin comment block (per-iteration section, upstream change summary) and align `.mstar/specs/spoke-adapter-architecture.md` §1.1/§5.2 pins in the same commit.

## Why This Matters

The npm/drift-gate lockstep and the `--all-targets` example compile are the two steps most easily missed; both fail late (CI / feature-on build) and cost a full round-trip.

## When to Apply

Any spoke version bump in the nexus workspace (lockstep policy, V1.139+).

## Examples

- V1.169 (0.10.0 → 0.11.1): additive `manifest.tools` — 4 literal sites (1 production builder + 3 feature-gated examples), npm + drift-gate lockstep, `tools.*` refusal pinning. 
