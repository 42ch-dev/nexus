---
title: "Complete host retirement: evidence-first disposition, closed-operation rule, and cohort collapse"
module: "apps/nexus42 + crates/nexus-daemon-runtime + crates/nexus-core"
date: "2026-09-21"
problem_type: architecture_pattern
category: architecture-patterns
severity: high
source_plan: "iteration:v1.193"
created_at: 2026-09-21
last_updated: 2026-09-21
tags:
  - retirement
  - host-removal
  - feature-cohorts
  - test-migration
  - evidence-before-retirement
  - cli
  - disposition-ledger
  - cargo-collapse
status: shipped
---

# Complete host retirement: evidence-first disposition, closed-operation rule, cohort collapse

## Context

v1.193 retired the legacy Rust HTTP daemon host (`nexus42 daemon` group,
`nexus-daemon-runtime` crate, web-embed/legacy-cli/basic-cli cohorts) in one
iteration while **preserving every retained domain capability** in nexus-core,
the TS service, and Electron. Three sequential plans (P0 direct-core authoring,
P1 operator/peer, P2 legacy-host removal), 34 tasks, tri-QC + targeted QA per
plan. This distills the method-level learnings; the per-leaf command inventory
is iteration history.

## Guidance

1. **Disposition ledger before deletion.** Inventory every CLI leaf from the
   clap parser + reachable handlers (never from `DaemonClient` import counts —
   imports include tests/types and overcount). Classify each row:
   `retain-direct` (already local), `retain-cloud` / `retain-connect` (remote
   transport is not local mediation), or `remove-cli`. Group rows only when
   every leaf shares one disposition.

2. **The closed-operation rule.** A public core method existing does NOT make
   a CLI operation complete. A leaf is retainable only if the core/local owner
   covers its **complete operation**: required production ports (a real
   `RunnerDeps`, not `RunnerDeps::default()`), the observation closure (a
   private capture drain a core integration test cannot reach), and any forced
   capability (forced SOUL synthesis needs a registry-backed synthesizer).
   Incomplete → delete the entrance, keep the library. Never invent an
   equivalent-TS-route claim without reading the route.

3. **Shared writers are real tasks.** Cross-plan shared files (`lib.rs`,
   top-level router, final Cargo) need named owner tasks with compile-ready
   boundaries — "companion patch" handoffs defer compilation debt to the
   review wave and break dependent-task readiness.

4. **Plan-level dependency gates.** Strict P0 → P1 → P2 with "successor starts
   only from the integrated Done baseline" prevented task-only cherry-picks.
   Cost is acceptable: the plans were genuinely sequential in surface.

5. **Cohort collapse honesty.** Final feature graph: app `default = ["cli"]`,
   independent `connect-host` (explicit spoke-adapter/compute — compute is
   real retained behavior, do NOT disable it for a prettier graph), core domain
   no-default-features engine-free, native/TS unchanged `["execution"]`.
   Prove each cohort with `cargo tree` pins + bin checks; a connect-only
   `--all-targets` row does not build (cli-cohort test targets import
   cli-gated modules) — use bin check + named tests.

6. **Test migration receipts.** Migrate nonduplicated behavior only
   (authorization, CAS/OCC, transitions, durable state, real failure/cleanup);
   HTTP envelopes, router registrations and boot-only wiring retire with the
   host. An assertion-level receipt (Migrated / Existing owner / Retired-host
   reason) per deleted file keeps the deletion auditable. wording-shape
   assertions (exact holder labels, typify DTO enum/maxLength shapes) retire;
   the behavior (lock conflict refused, oversized input rejected) migrates
   wording-independently.

7. **The migration surfaces pre-existing production defects.** Plan for a
   bounded in-task fix authorization when a migration makes a latent defect
   load-bearing (in v1.193: peer-disconnect eviction `Arc::ptr_eq` guard,
   compute-manifest polarity, settle-conditioned session advance). A scoped
   fix with the QC seat revalidating beats a residual that hides a broken
   preserved surface.

## Why This Matters

Retirement work fails most often by deleting capability with the entrance, or
by keeping entrances whose operation was already gone. The ledger +
closed-operation rule + receipt discipline make "what did we remove vs what do
we keep" auditable line by line — and make QC/QA evidence generation
mechanical.

## When to Apply

Any host/service/entrance retirement; any "delete the old path" iteration;
default-entry cutover with retained independent products.

## Examples

- `guides/command-disposition-ledger.md` (v1.193 package) — the row inventory.
- `guides/technical-contracts.md` (v1.193 package) — frozen signatures, the
  complete-operation table, neutral-owner moves, Cargo cohorts.
- CI: `rust-core-domain` job rows running per-target feature sets (a removed
  matrix row can silently orphan migrated coverage — give the migrated
  targets their own leg).
