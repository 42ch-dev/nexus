---
module: crates/nexus-moment-context-assembly + crates/nexus-spoke-adapter + apps/nexus42
date: 2026-09-02
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-09-02-v1.181-p1-lore-hygiene-and-pack-import
tags:
  - wire-contracts
  - inspector
  - deny-unknown-fields
  - additive-dto
  - closed-dto
  - regression
  - codegen
  - plan-writeback
applies_when:
  - "Adding a new top-level section to a packet/response that a daemon handler validates against a typed wire DTO"
  - "Judging whether an 'additive product-local section' counts as a wire-contract change"
  - "Writing a plan constraint like `wire_contracts_changed: false` that involves an enriched output packet"
  - "Recovering from a 500 caused by an unknown field on a deny_unknown_fields DTO"
---

# Additive Packet Sections Are Wire Changes (Closed-DTO Enforcement)

**Track**: Knowledge (durable guidance, distilled from v1.181 P1 T1).

## Context

The assembly-inspector packet pattern (V1.151, DF-76) lets MCA emit **additive product-local top-level sections** (`slot_map`, `budget`, `moment_directive`) alongside the spoke-derived `modules.*` vocabulary. Each such section is part of the daemon-served inspector packet — and the daemon handler validates the packet against the typed `MomentInspectResponse` DTO, whose schema uses `deny_unknown_fields` (closed field set: `budget`, `modules`, `moment_directive`, `slot_map`).

The v1.181 P1 Task 1 (DF-79 lore hygiene transforms) added a new additive `hygiene` trace section to the packet under a plan constraint `wire_contracts_changed: false`. The constraint was **wrong**: an additive top-level section on a closed DTO IS a wire change. Three `inspector_cli` E2E tests failed with daemon 500s — `unknown field 'hygiene', expected one of budget/modules/moment_directive/slot_map` — and the implementer's gate run had not covered the `inspector_cli` suite, so the regression shipped past the task gate and was caught only by a full `nexus42` test run at review time.

## Guidance

1. **An additive top-level section on a validated packet is a wire DTO change**, same class as adding a field: schema JSON + generated Rust/TS refresh + additive npm version bump (`@42ch/nexus-contracts` 0.31.0 → 0.31.1) + CHANGELOG. Follow the V1.151 precedent: field in schema `properties` (NOT `required`), `pnpm run codegen`, wire-drift gate green.
2. **When writing `wire_contracts_changed: false` in a plan**, enumerate every output packet the task touches and check each against its validating DTO's field set — "the section is additive product-local" does not imply "wire-free" when the consumer validates against a closed DTO. Related: `wire-contracts-frozen-verification.md` (8-point gate for the `false` claim).
3. **Write back the plan at discovery, then fix** (phase-gates discipline): the plan's Global Constraints was amended in place (`wire_drift` writeback documenting the locked fix shape) before the fix dispatch — not silently special-cased in code.
4. **Gate claims must cover the crate's E2E suites, not just the lib subset**: `cargo test -p nexus42` includes `inspector_cli`; an implementer gate run that passes lib tests but skips E2E suites leaves exactly this class of regression undetected. Reviewers should verify gate breadth when a change touches a daemon-served surface.
5. **Optional-by-construction keeps clients compatible**: the field is optional/absent-tolerant (`hygiene?: HygieneTraceEntry[]`), so existing consumers never see it when no transforms are configured.

## Why This Matters

The "additive section" framing suggested zero wire surface, so the plan banned the very change the DTO required, and the regression reached review. The cost was a fix wave + re-review; the prevention is a one-line check at plan-writing time: *which DTO validates this packet, and is its field set closed?*

## When to Apply

- Any enriched packet/response section added to a surface a handler validates with a typed DTO (inspector, run reports, export manifests).
- Plan-time `wire_contracts_changed` assessment for tasks that touch assembly-inspector or other enriched-packet surfaces.
- Post-merge audits: grep the diff for new top-level keys in packet builders, then check the validating schema's `properties`/`required`.

## Examples

- v1.181 P1 T1: `hygiene` section added to `build_inspector_packet` → 3 `inspector_cli` 500s → T1-fix added schema field + codegen + 0.31.1 bump; wire-drift gate became the regression guard (qc1 confirmed it as the "regression guard" for this exact class).
- V1.151 (DF-76) is the precedent for the sections themselves — each of `slot_map`/`budget`/`moment_directive` required the same DTO/schema treatment at ship time.