# Local API Surface Conventions

| Attribute | Value |
| --- | --- |
| **Status** | Normative — V1.65 Prepare amendment (V1.64 cursor/error/`items` conventions + chapter-content file-backed route rules) |
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
> `code`/`message`/`details` from `body.error`, not from the top level. V1.67
> P0 (FE1-ORCH) **resolves** the ad-hoc `(StatusCode, String)` bodies that
> remained in orchestration handlers (`schedules.rs` / `sessions.rs` /
> `presets.rs`): they are swept onto this canonical envelope via a shared
> error-mapping helper. After V1.67, ad-hoc tuple bodies are not an allowed
> Local API surface; clients no longer need the `http_<status>` fallback for
> orchestration routes.

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

> **V1.67 ratification (`@42ch/nexus-contracts` 0.5.0 → 0.6.0):** the canonical `NexusApiError::error_code()` module is aligned to lowercase `snake_case` **globally** — it previously emitted `UPPER_SNAKE_CASE`, contradicting this section. The ~18 canonical codes (e.g. `UNINITIALIZED`→`uninitialized`, `INVALID_INPUT`→`invalid_input`, `INTERNAL`→`internal`, `AUTH_REQUIRED`→`auth_required`, `NOT_FOUND`→`not_found`) change in the 0.6.0 bump. This is a global error-module change, not an orchestration-handler-local one.

---

## 4. List-array naming

`items` is canonical for plain list responses.

> **V1.67 ratification (F-P3, `@42ch/nexus-contracts` 0.5.0 → 0.6.0):** the prior legacy allowance for schema-backed plain lists is **removed**. V1.67 P0 renames the four remaining schema-backed per-resource keys to `items`:

| Key | Resource | V1.67 status |
| --- | --- | --- |
| `works` | Works list | **Renamed → `items`** (`GET /v1/local/works`). |
| `schedules` | Schedule list | **Renamed → `items`** (`GET /v1/local/orchestration/schedules`). |
| `sessions` | Orchestration sessions list | **Renamed → `items`** (`GET /v1/local/orchestration/sessions`). |
| `capabilities` | Capability registry list | **Renamed → `items`** (`GET /v1/local/orchestration/capabilities`). |
| `items` | chapters / findings / creators / workspace / KB / pending-review / agent-host | Already canonical; no change. |
| `embedded` / `system` / `user` | Preset management grouped response | Intentional **grouped** response, not a plain list — retained. |

Pre-1.0 → no compatibility shim. Hand-written / local-only plain lists not yet schema-backed (`references`, `worlds`, `fragments`, `pool entries`, orchestration `presets` DTO) are **out of the 0.6.0 bump**; they adopt `items` at their next schema-promotion or feature touch.

New endpoints MUST use `items`. UI adapters must not carry legacy-key normalization after 0.6.0 for the renamed endpoints.

---

## 5. Sort parameters

> **V1.67 lock (F-F1):** the prior `sort_by` + `sort_order` two-parameter sketch is **superseded** by a single optional `sort` query parameter.

List endpoints MAY expose a single optional `sort` query parameter.

Grammar:

```text
sort := term ("," term)*
term := ["-"] key
key  := endpoint-defined stable logical key
```

Examples:

```text
?sort=-updated_at
?sort=-updated_at,name
?sort=volume,chapter
```

Rules:

1. No `sort_by`, `sort_order`, `order`, `direction`, or camelCase variants on Local API query schemas.
2. `-key` means descending; `key` means ascending.
3. Each endpoint MUST publish its allowed keys in its query schema/spec.
4. Unsupported keys return the canonical error envelope with `code: "<resource>_sort_invalid"`.
5. Cursor-paginated endpoints MUST document which sort keys are compatible with their cursor. If arbitrary sorting would require a new cursor design, only the default sort is implemented and other keys are deferred in the endpoint spec.

V1.67 implements server-side sort on: works (`updated_at`/`title`/`status`/`intake_status`), schedules (`created_at`/`updated_at`/`status`/`preset_id`/`label`), sessions (`session_id`/`creator_id`/`preset_id`/`status`), capabilities (`name`). Chapters document/accept default `volume,chapter` only (cursor-keyed). Other lists adopt on next schema-promotion.

---

## 6. File-backed chapter-content routes

V1.65 introduces the chapter-content Local API surface under
`/v1/local/works/{work_id}/chapters/*`; detailed field contracts live in
[chapter-content-local-api.md](./chapter-content-local-api.md). This section is
the normative cross-surface convention for any Local API route that exposes
chapter outline/body files through DB-sourced paths.

### 6.1 Chapter lists use `items` + cursor from day one

`GET /v1/local/works/{work_id}/chapters` MUST use the canonical new-list shape:

```json
{
  "items": [],
  "pagination": {
    "limit": 50,
    "next_cursor": null,
    "has_more": false
  }
}
```

Do not introduce a `chapters` array key. Chapter list ordering defaults to
`volume ASC, chapter ASC`; optional filters such as `status` must preserve cursor
semantics and return `chapter_cursor_invalid` / validation errors for bad input.

### 6.2 Outline PUT is atomic file write + DB metadata update

`PUT /v1/local/works/{work_id}/chapters/{n}/outline` is the only writable
chapter-file route in V1.65. It MUST:

1. Load the target `outline_path` from `work_chapters` after Work ownership is
   verified. If absent, initialize the canonical seed path explicitly.
2. Resolve that path under the active workspace root and apply the path guard in
   §6.5 before creating directories or writing bytes.
3. Write to a sibling temp file, flush/sync, then atomically rename over the
   final outline file, mirroring the reconcile atomic write pattern in
   `work_chapters::sync_frontmatter_status`.
4. Update `work_chapters.outline_path` and `updated_at` in the same transactional
   finalization path as the file rename. Failed DB update or failed rename must
   clean up the temp file where possible and must not report success.
5. Return the committed outline DTO, not a speculative echo.

Successful outline PUT MUST NOT automatically change chapter `status`. Status is
author-controlled through the explicit chapter structure route.

### 6.3 Structure PATCH status and protection rules

`PATCH /v1/local/works/{work_id}/chapters/{n}` is the V1.65 structure-edit route
for metadata such as title/slug/planned word count/volume/status.

Normative rules:

- The only normal UI-driven status progression in V1.65 is
  `not_started → outlined`.
- Reverse transitions and terminal-state changes MUST be explicit actions with a
  reason if implemented; they must never occur as side effects of outline PUT or
  ordinary structure edits.
- `draft` chapters may be structurally edited, but consumers should warn that a
  body already exists.
- `finalized` structural edits require explicit confirmation from the caller.
- `published` structural edits are hard-blocked in V1.65 unless a future
  publish-retraction design changes the policy.
- Chapter deletion is out of scope for V1.65; any future delete route MUST
  hard-block `finalized` and `published` chapters.

### 6.4 Body markdown is read-only in V1.65

`GET /v1/local/works/{work_id}/chapters/{n}/body` may return body markdown and
optional parsed frontmatter for rendering, but V1.65 MUST NOT add a body write
route. The orchestration host-tool path remains the body writer until V1.66
designs a per-chapter edit lock and conflict policy.

### 6.5 DB-sourced path guard is mandatory

`outline_path` and `body_path` are DB-sourced and must be treated as untrusted
until resolved. Every outline/body read or write route MUST:

1. Join the relative DB path to the daemon workspace root.
2. Canonicalize the workspace root.
3. For existing targets, canonicalize the target and require it to start with
   the canonical workspace root.
4. For missing-but-creatable targets, validate the normalized target or nearest
   existing parent cannot escape the workspace root before creating directories.
5. Reject traversal/symlink escape with a stable validation error, not by falling
   through to an arbitrary filesystem error.

This mirrors the W-002 defense-in-depth guard in
`host_tool_handlers.rs` for body reads and applies it to the new outline PUT.

### 6.6 Soft concurrency semantics

V1.65 has no hard per-chapter edit lock. Outline writes are last-write-wins at
the file level, and orchestration reads the outline at draft-time as a natural
snapshot. UI consumers should warn when editing outlines for `draft` or
`finalized` chapters.

The daemon **does** acquire the per-Work runtime lock for `PUT outline` and
`PATCH structure` to honor the existing single-writer invariant
(`multi-work-lifecycle.md` §4.2). The lock is released on both success and
error paths. This is implementation-specific and does not change the contract's
last-write-wins semantics for clients.

---

## 7. Handler/schema drift closure

Handlers that serve schema-promoted Local API routes MUST emit `generated::local_api::*` response shapes or structurally equivalent types verified by `schema_drift_detection.rs` in `CheckMode::Strict`.

Future endpoint acceptance requires:

1. JSON Schema under `schemas/local-api/<resource>/` (or `local-api/common/` for shared envelopes).
2. Codegen output committed for Rust and TypeScript.
3. Handler response body aligned to generated Rust DTOs.
4. Strict drift detection coverage.
5. README inventory updated under the affected schema subtree.

---

## 8. Evidence and V1.64 decisions

The V1.63 Local API surface audit identified:

- F-P1: Works uses offset/limit while peers use cursor pagination.
- F-P2: Findings list endpoint lacks a response schema.
- F-P3: List array keys differ across resources.
- F-E1: No standardized error envelope.
- F-F1: Sort parameters are not standardized.

V1.64 closes F-P1, F-P2, and F-E1 for the Web UI data-layer baseline, while documenting F-P3 and F-F1 as future conventions with adapter coverage for MVP.

---

## 9. Local daemon port discovery (V1.66 desktop shell)

Local API clients that connect over loopback HTTP use a **resolved daemon base URL**, not a schema-defined discovery endpoint. (Compass: [v1.66 §5 #3 LOCKED](../iterations/v1.66-tauri-desktop-shell-delivery-compass-v1.md).)

V1.66 desktop-shell convention:

1. Default port is `8420` (the `boot.rs` default).
2. `NEXUS_DAEMON_PORT` may override the default when the client/launcher environment provides it.
3. The desktop launcher passes the resolved port explicitly to the sidecar:
   ```text
   nexus42 daemon start --foreground --port <resolved_port>
   ```
   so CLI args and environment cannot diverge.
4. Readiness is confirmed by:
   ```text
   GET http://127.0.0.1:<resolved_port>/v1/local/runtime/health
   ```
   (NOT stdout parsing — see [daemon-runtime.md](daemon-runtime.md) §12.2).
5. Clients MUST treat health-probe failure as transport/lifecycle failure, not as a schema mismatch.
6. **V1.66 does not introduce a dynamic port handshake endpoint or daemon-lifecycle Local API schema** (`wire_contracts_changed: false`).

If a future iteration introduces dynamic port allocation, it must define a separate launcher-to-app handshake contract **before** adding any Local API schema.
