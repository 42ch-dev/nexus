---
module: tooling/codegen (rust-gen, ts-gen), crates/nexus-contracts
date: 2026-10-07
problem_type: tooling_decision
category: tooling-decisions
severity: low
plan_id: 2026-10-07-v1.207-p1-codegen-contract-debt
applies_when:
  - "changing the codegen lane (rust-gen / ts-gen / schema sources)"
  - "regenerating contracts through pnpm run codegen"
  - "adding a schema whose $ref targets are shared across response modules"
tags:
  - codegen
  - rust-gen
  - ts-gen
  - typify
  - prettyplease
  - ref-dedupe
  - byte-stability
  - const-true
related_components:
  - tooling/codegen
  - nexus-contracts
last_updated: 2026-10-12
---

# Codegen lane: single-home emission, in-process formatting, byte-stability

> **Status: current.** The v1.207 P1 convergence delivered single-home emission + in-process formatting; **v1.214 P1** closed the abort-window residual (`R-V1190-RUSTGEN-PRETTY-RESET`) with a recoverable stage-and-swap publication plus a `mkdir`-only per-output-directory publication lock, and made the TS declaration scanner string/template/comment aware (`R-V1190-CODEGEN-TS-BRACE-COUNT`).

## Context

The v1.207 P1 convergence retired the v1.190 codegen debts: regenerated
`core_service_discovery.rs` was an irreproducible single-line token dump,
`$ref` targets were re-emitted inside response modules (dual import paths,
consumer-side qualification workarounds in business code), and the lane had
pruned ~280 generated files mid-write once. The lane is shared by every
contract consumer, so its discipline is load-bearing.

## Guidance

1. **Single-home emission rule** — each `$ref` target type is emitted exactly
   once, in the module that *declares* the name (root == name, or
   `name.startsWith(root)` for nested inlined types). Both generators
   (`rust-gen/src/main.rs`, `ts-gen.ts`) remove the inline copy; consumers
   import from the one canonical module. Names with no unambiguous owner stay
   untouched (conservative).
2. **In-process formatting** — rust-gen parses generated modules with `syn`
   and re-prints with `prettyplease`. Do **not** depend on a rustfmt binary
   or a toolchain pin: the verify-codegen CI job installs no rustfmt.
3. **Write discipline (stage-and-swap; v1.214)** — every module is generated,
   parsed and rendered BEFORE the first write (failure paths write nothing), and
   publication is a **recoverable replacement**: the committed tree is renamed to
   a sibling `<tree>.backup`, the completed `<tree>.staging` is renamed into its
   place, and the backup is dropped last; `recover_interrupted_publication`
   restores a backup left by a crashed run before the next build. This is not
   atomic — a hard kill between the two renames leaves the output path absent
   until a later run recovers it (the module docs say so explicitly).
4. **Byte-stability verification** — the acceptance proof is: run the lane
   twice, compare a full-tree `sha256` manifest (identical) plus an empty
   `git status`; pin the historical target file by recorded sha. Ordinary-path
   byte-stability is not proof for the abort window above.
5. **Literal-`const` schema markers** — hooks that enforce `const: true` can
   be attached to the *wrong field* by an off-by-one in the generator and
   silently validate nothing. Pin the generated type with **bidirectional**
   regressions: deserialization rejects `false` AND the outbound path cannot
   emit it (v1.207 P2 added a generated `serialize_with` guard for this).
6. **Publication lock — one atomic primitive, no stale takeover (v1.214)** —
   the TS lane serializes the whole recover→build→publish lifecycle per output
   directory with a single `mkdir(<outDir>.lock)` claim: `EEXIST` fails visibly
   (naming the lock path and the recorded owner), and only the claim that still
   owns the lock releases it. Automatic stale recovery is deliberately **absent**:
   four review rounds showed every read-then-delete recovery design (age-out
   gate, atomically published claim + `linkSync`, liveness-checked
   `rename(...).take` takeover) leaves another ownership window through which two
   runs can publish an incomplete tree. An orphaned lock is removed manually.
   Reintroducing an automatic reclaim path requires a proof-level argument, not
   a heuristic.

## Why This Matters

- Dual emission forces consumer-side qualification hacks, which then leak
  into every downstream crate that touches generated DTOs.
- Non-reproducible generated files make "run the lane" unsafe and destroy
  reviewability of codegen changes (the diff becomes noise).
- Silent marker mis-attachment produces green tests over unenforced contracts.

## When to Apply

- Any change to the generators or schema sources; any lane run.
- Adding a schema that references types already emitted in sibling response
  modules (watch for the inline-copy regression).
- Any new `const`/literal marker in schemas.

## Examples

### Before — response-scoped qualification workaround

```rust
// consumer forced to qualify because two modules declared the same name
let entry: crate::generated::works::work_pool_entry::WorkPoolEntry = ...;
```

### After — single canonical import

```rust
use crate::generated::works::work_pool_entry::WorkPoolEntry;
// the response module no longer declares its own copy
```

Source: v1.207 P1 iteration (PR #367) — task-1/2 reports; in-repo
operational notes: `tooling/codegen/README.md` + `tooling/codegen/rust-gen/AGENTS.md`.
