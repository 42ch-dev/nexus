# Local API Surface Conventions

| Attribute | Value |
| --- | --- |
| **Status** | Draft (V1.64) — conventions established by Track B this iteration; promote to `Normative — V1.64 Shipped` at P-last once handlers conform |
| **Document class** | Master |
| **Scope** | Cross-resource Local API response/query conventions for schemas under `schemas/local-api/` and handlers under `nexus-daemon-runtime` |
| **Coordinates with** | [schemas-directory-layout.md](./schemas-directory-layout.md), [schemas-external-consumer-boundary.md](../schemas-external-consumer-boundary.md), [daemon-runtime.md](./daemon-runtime.md) |
| **Evidence** | [surface-audit.md](../../plans/reports/2026-06-24-v1.63-local-api-orchestration-and-preset-dtos/surface-audit.md) |

---

## 1. Purpose

The Local API is now consumed across a language boundary by generated TypeScript contracts and the bundled local Web UI. This Master records cross-resource conventions so future `/v1/local/*` endpoints do not reintroduce handler DTO drift or per-resource shape divergence.

This document is intentionally about **surface conventions**. Field-level entity semantics remain in the per-resource schemas under `schemas/local-api/` and the owning domain specs.

---

## 2. Pagination

### 2.1 Canonical pattern — cursor pagination

New list endpoints MUST use cursor-based pagination:

```json
{
  "items": [],
  "pagination": {
    "next_cursor": "opaque-or-null",
    "has_more": false
  }
}
```

Request/query parameters:

| Parameter | Type | Rule |
| --- | --- | --- |
| `cursor` | string, optional | Opaque cursor returned by the previous response. Clients MUST NOT parse it. |
| `limit` | integer, optional | Bounded by the handler; default should be resource-appropriate. |

Response:

| Field | Type | Rule |
| --- | --- | --- |
| `items` | array | Canonical list array key for all new endpoints. |
| `pagination.next_cursor` | string or null | Non-null only when another page exists. |
| `pagination.has_more` | boolean | True when the client may request another page. |

`PaginationInfo` is the canonical shared shape. Reuse an existing generated `PaginationInfo` schema where the resource already exposes one; otherwise define it in the closest appropriate `schemas/local-api/<resource>/` folder until a shared common pagination schema is promoted.

### 2.2 Legacy pattern — offset/limit

Offset/limit plus `total` is legacy. It exists because Works was the first promoted Local API list surface. V1.64 migrates Works pagination to cursor semantics (F-P1). New endpoints MUST NOT introduce offset/limit pagination.

When converting a legacy endpoint:

1. Replace `offset` with `cursor`.
2. Keep `limit` if needed.
3. Return `pagination: PaginationInfo { next_cursor, has_more }`.
4. Coordinate any response array rename separately if it would break existing consumers.

---

## 3. Error envelope

### 3.1 Canonical wire shape

All Local API JSON error responses are emitted by the daemon runtime as a
**wrapped envelope** (see `ApiErrorResponse` in
`crates/nexus-daemon-runtime/src/api/errors.rs`):

```json
{
  "success": false,
  "error": {
    "code": "work_not_found",
    "message": "Work not found. Check the Work ID and try again.",
    "details": { "work_id": "..." },
    "request_id": "req_01h..."
  }
}
```

- `success`: always `false` for errors.
- `error`: the canonical detail object. The inner `{ code, message, details? }`
  is what the shared `ErrorResponse` schema models
  (`schemas/local-api/common/error-response.schema.json`); the schema describes
  the **inner detail**, not the full wire body.
- `error.request_id`: correlation ID injected by the request-tracing middleware
  when active (`crates/nexus-daemon-runtime/src/api/middleware.rs`); absent when
  the middleware is not installed. It lives under `error`, not at the top level.

Inner detail fields:

| Field | Required | Rule |
| --- | --- | --- |
| `code` | yes | Stable, machine-readable string. |
| `message` | yes | Human-readable and actionable. |
| `details` | no | JSON object for structured values such as IDs, validation paths, or field names. Do not put unstructured stack traces here. |
| `request_id` | no | Correlation ID; set by middleware, not by handlers. |

> **Implementation note:** transport adapters and consumers MUST read
> `code`/`message`/`details` from `body.error`, not from the top level. A
> handful of orchestration handlers still return ad-hoc `(StatusCode, String)`
> bodies (deferred under `R-V164-FE1-ORCH`); those do not carry a structured
> code and clients must fall back to `http_<status>`.

### 3.2 Error-code naming

Error codes use lowercase snake_case:

```text
<resource>_<failure>
```

Examples:

| Scenario | Code |
| --- | --- |
| Missing Work | `work_not_found` |
| Invalid preset YAML | `preset_invalid` |
| Findings list cursor rejected | `finding_cursor_invalid` |
| API key missing or invalid | `auth_invalid` |
| Workspace boundary rejected | `workspace_path_forbidden` |

Use singular resource nouns (`work`, `preset`, `finding`, `workspace`). For cross-resource failures, use the subsystem noun (`auth`, `runtime`, `schema`, `validation`). Codes are contract surface: change only with a schema/version bump and consumer coordination.

---

## 4. List-array naming

`items` is canonical for new list responses.

Legacy keys remain until coordinated breaking sweeps:

| Legacy key | Resource | Status |
| --- | --- | --- |
| `works` | Works list | Legacy; Works cursor migration in V1.64 does not require array rename unless explicitly coordinated. |
| `schedules` | Schedule list | Legacy. |
| `sessions` | Orchestration sessions list | Legacy. |
| `capabilities` | Capability registry list | Legacy. |
| `embedded` / `system` / `user` | Preset management grouped response | Intentional grouped response; not a plain list. |

F-P3 (array-key unification) is deferred because renaming existing response arrays is a multi-handler breaking change. New endpoints, including `list-findings-response`, MUST use `items`.

UI adapters may normalize legacy keys to `items` internally, but handlers and schemas remain the wire SSOT.

---

## 5. Sort parameters

Sorting is a future convention (F-F1), not a V1.64 implementation requirement. When a list endpoint adds sorting, use:

| Parameter | Type | Rule |
| --- | --- | --- |
| `sort_by` | string | Resource-defined field name or stable logical sort key. |
| `sort_order` | `"asc" | "desc"` | Direction; default is resource-specific but must be documented in the query schema. |

Do not introduce alternate names such as `order`, `direction`, `sort`, or `sortDirection` on Local API query schemas. Unsupported sort keys should return `ErrorResponse { code: "<resource>_sort_invalid", ... }`.

---

## 6. Handler/schema drift closure

Handlers that serve schema-promoted Local API routes MUST emit `generated::local_api::*` response shapes or structurally equivalent types verified by `schema_drift_detection.rs` in `CheckMode::Strict`.

Future endpoint acceptance requires:

1. JSON Schema under `schemas/local-api/<resource>/` (or `local-api/common/` for shared envelopes).
2. Codegen output committed for Rust and TypeScript.
3. Handler response body aligned to generated Rust DTOs.
4. Strict drift detection coverage.
5. README inventory updated under the affected schema subtree.

---

## 7. Evidence and V1.64 decisions

The V1.63 Local API surface audit identified:

- F-P1: Works uses offset/limit while peers use cursor pagination.
- F-P2: Findings list endpoint lacks a response schema.
- F-P3: List array keys differ across resources.
- F-E1: No standardized error envelope.
- F-F1: Sort parameters are not standardized.

V1.64 closes F-P1, F-P2, and F-E1 for the Web UI data-layer baseline, while documenting F-P3 and F-F1 as future conventions with adapter coverage for MVP.
