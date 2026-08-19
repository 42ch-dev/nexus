---
title: Field-level error envelope for generated DTO write surfaces
category: api-design
track: knowledge
source_plan: 2026-08-19-v1.169-p1-rules-write-api
source_iteration: V1.169
created: 2026-08-19
last_updated: 2026-08-19
status: active
---

# Field-Level Error Envelope for Generated-DTO Write Surfaces

The daemon-API pattern for mutation routes whose clients are **forms**: every form-producible failure must surface as a field-level error the form can render next to the input that caused it — never as an extractor rejection, a raw JSON blob, or a CLI-flag message.

## Context

V1.166 shipped world-rules read-only; V1.169 P1/P2 added the write surface (DF-82). The tension: DTO schemas are generated (`nexus-contracts` codegen, `Json<DTO>` extractor), but schema-level value constraints (`minLength`/`enum`/`format`) fail at the **extractor** with the framework's default rejection body — which a form cannot map onto fields.

## Guidance

1. **Lenient schemas, handler-side value validation.** Schema files constrain **type structure only** (`additionalProperties: false`, required lists, types). All value rules (trim-empty, closed status vocab, `min ≤ max`, operand-form) run in the handler after extraction. This is a deliberate divergence from older schemas that predate the form-consumer product lock.
2. **Closed field vocabulary.** Reject via the existing `InvalidInput { field, reason }` → 400 `invalid_input`, `error.details = { field, reason }`. Define the `field` vocabulary as a **closed table** the UI maps onto form inputs 1:1 — nested members mirror the validator's member names exactly (`constraint.family`, `constraint.module_key`, `constraint.min`, …) so the form needs zero translation. Validation-server errors that don't address a field (e.g. empty PATCH) get a synthetic key (`patch`).
3. **Member-aware validator seam, one grammar.** When the shared validator returns string errors, add an additive member-aware variant (`parse_carrier_json_member -> CarrierError { member, reason }`) and delegate the legacy function to it (`map_err(|e| e.reason)`) — CLI messages stay byte-identical; the API projects `format!("{}.{}", prefix, member)`. Never string-sniff messages at the call site.
4. **Validation order is contract.** Addressing errors (404, no cross-world existence leak) precede payload errors on PATCH; a foreign id must learn nothing about validation behavior. Pin it with a **discriminating test**: invalid payload → unknown/cross-world id must still 404, not 400.
5. **Fail-closed storage corruption.** Malformed stored JSON on a mutation path → 500 `internal`, no write; pin with a seeded-corruption test.
6. **Schema-level null-union when absence must differ from empty** (typify quirk, see [codegen-optional-field-callsite-coverage.md](../engineering/codegen-optional-field-callsite-coverage.md)): typify collapses optional arrays (absent ≡ `[]`); a `["array","null"]` union on the update schema yields `Option<Vec<_>>` where absent/null = unchanged and explicit `[]` = meaningful clear.

## Why This Matters

The error surface **is** the form UX. Extractor rejections and blob errors force the UI to invent a second grammar or dead-end the author — exactly the non-engineer blocker (DF-B-02) the write surface exists to remove.

## When to Apply

Any new daemon mutation route consumed by a Control Room form (CRUD panels, settings writers), and any shared validator reused across CLI + API.

## Examples

- World rules write API (V1.169 P1): 13-row closed vocabulary, member-aware `parse_carrier_json_member`, 404-before-payload, `[]`-clear null-union. QC/QA evidence: `.mstar/sdd/2026-08-19-v1.169-p1-rules-write-api/` (local harness).
