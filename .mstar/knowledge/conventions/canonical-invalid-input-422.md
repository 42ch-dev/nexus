---
module: apps/nexus-service
date: 2026-09-07
last_updated: 2026-10-12
problem_type: convention
category: conventions
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
  - spec-alignment
related_components:
  - apps/nexus-service
  - nexus-local-db
  - schemas
  - nexus-contracts
---

# Invalid-input status is chosen by the producer, not by the code

## Context

The v1.185 Actor plan drafts used `400 invalid_input` in three places — §11.1 (invalid wire structure), §11.5 (World-owned summary reject), §11.6 (Creator/legacy `remember:true` reject) — while the canonical mapping for the stable code `invalid_input` was `422 Unprocessable Entity`. All three were reconciled to `HTTP 422 invalid_input` before implementation. The drift was possible because the **same stable code can render at two different statuses**, chosen by the producer that emits it, and because spec prose and the error-mapping table are separate documents that are not mechanically linked.

The mapping SSOT is `apps/nexus-service/src/errors.ts`. Read `statusForCode` (`errors.ts:58-64`), the `STATUS_BY_CODE` table (`errors.ts:27-56`), and `mapNativeError` (`errors.ts:83-109`) — not your memory of a spec line — before coding a status code. There is no `status_code()`/`error_code()` match in this service; migrated native codes arrive as generated `CoreError` JSON envelopes.

## Guidance

### 1. The current mapping: table default, explicit `http_status` wins

- `mapNativeError` prefers an explicit `wire.http_status` carried in the generated `CoreError` envelope and only falls back to `statusForCode(wire.code)` when the producer did not set one: `wire.http_status ?? statusForCode(wire.code)` (`errors.ts:98`). The schema allows `http_status` as an optional integer `400..599` (`schemas/core/core-error.schema.json`).
- `statusForCode` reads the `STATUS_BY_CODE` table verbatim for a known code; unknown codes are `500`, with two explicit overrides: `route_not_migrated` → `501` and `input_too_large` → `413` (`errors.ts:58-64`).
- In that table `invalid_input` is **400** (`errors.ts:29`). A semantic-validation failure that must surface as **422** therefore has to be produced either by a code whose table entry is 422 — `world_kb_validation`, `outline_validation_failed`, and the `compute_*` family are 422 (`errors.ts:37,46,52`) — or by a producer that stamps explicit `http_status: 422`. The stable code alone does not decide the status.
- Database faults and the sanitization boundary stay `500 internal`: `mapNativeError` replaces `internal` with a fixed public message and never leaks the native detail (`errors.ts:87-91,105-107`). `409` is reserved for coded conflicts; `404` for foreign/missing resources (without exposing another owner's data).

### 2. The trap: the same code at two statuses

`invalid_input` is not "the 422 code". A handler that returns `invalid_input` from the default table path renders **400**; one that returns it through a producer carrying `http_status: 422` renders 422. Same code, two statuses — chosen by producer, not by name.

- For a **new** endpoint whose spec prose says `HTTP 422 invalid_input`, either route the failure through a producer that sets explicit `http_status: 422`, or reconcile the spec to the status the producer actually renders. Do not assume the table's `invalid_input` entry is 422.
- Do **not** "fix" a surface whose spec says 400; verify **by producer + status**, not by code name.

### 3. JSON-Schema maxLength is codepoints; byte caps are runtime-enforced

JSON Schema `maxLength` counts Unicode codepoints. The durable cap on actor knowledge `summary` is **65,536 UTF-8 bytes** (spec §11.5). A schema can declare `"maxLength": 65536` as a coarse gate, but the authoritative check is runtime: `summary.len() > ACTOR_KNOWLEDGE_SUMMARY_MAX_UTF8_BYTES (65_536)` in `crates/nexus-local-db/src/actor_knowledge_store.rs:14` (Rust `len()` is bytes; enforced by `validate_summary_utf8_bytes`, `:85`). With multi-byte codepoints (CJK ≈ 3 bytes/codepoint), a schema-valid 65,536-codepoint string is byte-invalid.

- When a spec names a byte limit, the schema field can carry a matching `maxLength` but the runtime must enforce the byte bound; document both, and let the runtime check be what a client observes.
- Reuse the same rule wherever a stored or persisted text is bounded: check bytes in storage (surfaced as a native validation code, not only at the schema boundary).

### 4. Manual query parsing where a framework extractor would bypass the envelope

DELETE `/v1/daemon/characters/{character_id}/knowledge/{entry_id}` requires `expected_revision` in the query string. The handler parses it manually from `URLSearchParams` and throws the canonical envelope itself — `deleteKnowledgeEntry` (`apps/nexus-service/src/actors.ts:242-253`): a missing param is `HttpError(400, 'invalid_input', 'expected_revision is required')`, a non-integer/negative one is `HttpError(400, 'invalid_input', 'expected_revision must be an integer')`, before `core.deleteKnowledgeEntry(principal, …, parsed)`.

- For query-parameter positions with wire-contract semantics (required CAS tokens, closed param sets), parse the raw query string when you need envelope-consistent errors; do not rely on a framework default rejection body.
- Error `details` remain a closed vocabulary mapped 1:1 onto what a caller can fix; no raw framework text.
- **Never pre-filter a caller's members before a strict decode.** A transport that quietly drops unknown query keys (or unknown body members) before handing the payload to a strict decoder converts a rejection into a confident wrong answer: an unsupported filter or a typo returns a page that *looks* correctly filtered, and the strict decoder never sees the key it exists to refuse. Refuse unknown members explicitly at the boundary where the closed key set is known — `refuseUnknownQueryKeys(search, allowed, status)` (`apps/nexus-service/src/world-kb.ts:199`) is the current shape, and callers pass the status deliberately (e.g. `compute.ts:98` passes 400, `compute.ts:125` passes 422) instead of inheriting a helper default.

### 5. Spec-prose drift reconciliation: check the table against errors.ts

Before implementing any status code from spec prose:

1. Grep the spec section for status literals (`400`, `422`, `409`, …) and enumerate the claims.
2. Compare each against `errors.ts` `STATUS_BY_CODE` and the `http_status` the emitting producer stamps.
3. Reconcile the spec (or the code) with an explicit alignment commit in the same iteration, named by section (§11.1, §11.5, §11.6 alignments in v1.185 are the template).
4. Match by code + producer/`http_status` together: code alone is not proof of status.

## Why this matters

Clients branch on the stable code, but tests, SDKs, and ops dashboards frequently assert (or normalize on) the status. A same-code/different-status split makes a handler look spec-compliant while a status-asserting test fails; a spec that says the wrong status for a code invites a second implementation that "fixes" the mismatch by inventing a code. Codepoint-vs-byte caps accept schema-valid-but-oversized persisted text, which then surfaces as `500` deep in storage or a corrupt digest. Envelope-bypassing extractor rejections hide the error contract from callers that already parse the envelope.

## When to apply

- Adding or editing a daemon API route or tool handler that validates client input.
- Authoring or reviewing a spec section with HTTP status prose.
- Adding a schema limit or any persisted-length bound.
- Reviewing an error mapping where the same code appears at different statuses depending on the producer.

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

Durable authority: [Actor Product Model §11.1/§11.5/§11.6](../../specs/architecture/actor-product-model.md); mapping: `apps/nexus-service/src/errors.ts`. Related: [conventions/cli-surface-honesty-discipline](../conventions/cli-surface-honesty-discipline.md) (render the actual error code/status, never OR-match status strings), [api-design/field-level-error-envelope-for-generated-dtos.md](../api-design/field-level-error-envelope-for-generated-dtos.md) (closed `details` vocabulary, member-aware validation).
