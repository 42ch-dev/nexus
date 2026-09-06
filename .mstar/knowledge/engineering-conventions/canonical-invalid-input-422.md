---
module: nexus-daemon-runtime-api
date: 2026-09-07
problem_type: convention
category: engineering-conventions
severity: high
plan_id: 2026-09-06-v1.185-p3-run-memory-capture
applies_when:
  - adding-daemon-api-routes
  - writing-error-mapping
  - authoring-spec-status-codes
  - adding-request-schema-limits
tags:
  - error-mapping
  - http-status
  - invalid-input
  - 422
  - json-schema
  - utf-8
  - axum
  - spec-alignment
related_components:
  - nexus-daemon-runtime
  - nexus-local-db
  - schemas
  - nexus-contracts
---

# Canonical invalid-input → HTTP 422

## Context

The v1.185 Actor plan drafts used `400 invalid_input` in three places — §11.1 (invalid wire structure), §11.5 (World-owned summary reject), §11.6 (Creator/legacy `remember:true` reject) — while the daemon's canonical mapping for the stable code `invalid_input` is `422 Unprocessable Entity`. All three were reconciled to `HTTP 422 invalid_input` before implementation. The drift was possible because the codebase carries **two constructors that emit the same stable code with different statuses**, and because spec prose and the error-mapping table are separate documents that are not mechanically linked.

The mapping SSOT is `crates/nexus-daemon-runtime/src/api/errors.rs` — read its `status_code()` and `error_code()` match arms, not your memory of a spec line, before coding a status code.

## Guidance

### 1. Canonical: stable code `invalid_input` maps to 422

- Semantic validation of client input (bad shape, wrong enum, out-of-range value, disallowed field on a closed schema) is `422 UNPROCESSABLE_ENTITY` with the stable code `invalid_input` in the `{ error: { code, message, details?, request_id } }` envelope. Codes, not message text, are the branching contract; codes group at a coarse level and are stable across versions.
- Canonical constructors:
  - `NexusApiError::BadRequest { code: "invalid_input", message }` — the `"invalid_input"` arm of `status_code()` returns `UNPROCESSABLE_ENTITY` and `error_code() == "invalid_input"`.
  - `LocalDbError::ValidationError(msg)` converts to `BadRequest { code: "invalid_input" }` (same 422 path), so storage-level validation shares the handler-level semantics.
  - `NexusApiError::InputValidationFailed { details }` — `error_code() == "invalid_input"`, status 422, with per-entry `details.invalid_entries` (compute input manifests).
- Database faults and unexpected failures stay `500 internal`; `409` is reserved for coded conflicts; `404` for foreign/missing resources (without exposing another owner's data).

### 2. The trap: `NexusApiError::InvalidInput { field, reason }` is 400

The legacy `InvalidInput` variant (`field`/`reason`, `details = { field, reason }`) also reports `error_code() == "invalid_input"` but maps to **400 BAD_REQUEST** (`status_code()` arm; pinned by the `invalid_input_maps_to_400` test; legacy surfaces such as workspace init's empty-path 400). Same code, two statuses, chosen by constructor.

- For **new** endpoints, use the code-based form (`BadRequest { code: "invalid_input", .. }` or the `ValidationError` conversion) so spec prose `HTTP 422 invalid_input` and wire behavior agree.
- Do **not** "fix" the legacy variant — it keeps 400 for its existing surfaces. Do not copy it into a new handler and then claim the spec's 422 status; verify by constructor, not by code name.

### 3. JSON-Schema maxLength is codepoints; byte caps are runtime-enforced

JSON Schema `maxLength` counts Unicode codepoints. The durable v1.185 cap on knowledge `summary` is **65,536 UTF-8 bytes** (spec §11.5). The schema declares `"maxLength": 65536` as a coarse gate, but the authoritative check is runtime: `summary.len() > ACTOR_KNOWLEDGE_SUMMARY_MAX_UTF8_BYTES (65_536)` (Rust `len()` is bytes) in `crates/nexus-local-db/src/actor_knowledge_store.rs`. With multi-byte codepoints (CJK ≈ 3 bytes/codepoint), a schema-valid 65,536-codepoint string is byte-invalid.

- When a spec names a byte limit, the schema field can carry a matching `maxLength` but the runtime must enforce the byte bound; document both, and let the runtime check be what a client observes.
- Reuse the same rule wherever a stored or persisted text is bounded: check bytes in storage (`LocalDbError::ValidationError` → the canonical 422 path), not only at the schema boundary.

### 4. Manual query parsing where Axum extractors bypass the envelope

DELETE `/v1/daemon/characters/{character_id}/knowledge/{entry_id}` requires `expected_revision` in the query string. A typed `Query<T>`/`Path<T>` extractor rejection is framework-shaped (its own 400/404 body) and does not carry the canonical envelope. The handler therefore parses the raw `Uri` query manually — `parse_delete_expected_revision(&Uri)` in `crates/nexus-daemon-runtime/src/api/handlers/actor_knowledge.rs` — producing `knowledge_delete_invalid_input(...)` → 422 `invalid_input` with precise reasons: missing, duplicate param, unexpected key, non-integer, out of range.

- For query-parameter positions with wire-contract semantics (required CAS tokens, closed param sets), parse the raw query string when you need envelope-consistent errors; do not rely on the extractor's default rejection.
- Error `details` remain a closed vocabulary mapped 1:1 onto what a caller can fix (`field`/`reason` or `invalid_entries`); no raw framework text.

### 5. Spec-prose drift reconciliation: check the table against errors.rs

Before implementing any status code from spec prose:

1. Grep the spec section for status literals (`400`, `422`, `409`, …) and enumerate the claims.
2. Compare each against `errors.rs` `status_code()`/`error_code()` arms and the constructor used by the handler.
3. Reconcile the spec (or the code) with an explicit alignment commit in the same iteration, named by section (§11.1, §11.5, §11.6 alignments in v1.185 are the template).
4. Match by code + constructor together: code alone is not proof of status.

## Why this matters

Clients branch on the stable code, but tests, SDKs, and ops dashboards frequently assert (or normalize on) the status. A same-code/different-status split makes a handler look spec-compliant while a status-asserting test fails; a spec that says 400 for what the mapping renders as 422 invites a second implementation that "fixes" the mismatch by inventing a code. Codepoint-vs-byte caps accept schema-valid-but-oversized persisted text, which then surfaces as `500` deep in storage or a corrupt digest. Envelope-bypassing extractor rejections hide the error contract from callers that already parse the envelope.

## When to apply

- Adding or editing a daemon API route or tool handler that validates client input.
- Authoring or reviewing a spec section with HTTP status prose.
- Adding a schema limit or any persisted-length bound.
- Reviewing an error mapping where the same code appears under different constructors.

## Examples

### Before (spec prose drift — v1.185 drafts)

```markdown
- An empty patch is `400 invalid_input`.
- `summary` on World-owned create rejects `400 invalid_input`.
- Creator/legacy sessions reject `400 invalid_input` before Host execution.
```

### After (canonical, as aligned)

```markdown
- An empty patch is HTTP 422 `invalid_input`.
- `summary` on World-owned create rejects HTTP 422 `invalid_input`.
- Creator/legacy sessions reject HTTP 422 `invalid_input` before Host execution.
```

Durable authority: [Actor Product Model §11.1/§11.5/§11.6](../../specs/actor-product-model.md); mapping: `crates/nexus-daemon-runtime/src/api/errors.rs`. Related: [conventions/cli-surface-honesty-discipline](../conventions/cli-surface-honesty-discipline.md) (render the actual `error_code()`, never OR-match status strings), [api-design/field-level-error-envelope-for-generated-dtos.md](../api-design/field-level-error-envelope-for-generated-dtos.md) (closed `details` vocabulary, member-aware validation).
