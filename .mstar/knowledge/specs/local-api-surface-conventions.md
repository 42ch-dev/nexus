# Local API Surface Conventions

| Attribute | Value |
| --- | --- |
| **Status** | Normative — V1.74 amendment (§7.6 World KB relationship patch route extending the V1.73 World KB route pattern; additive relationship DTOs; per-row OCC with `expected_version`/`version` against `kb_relationships.revision`). Prior: V1.73 amendment (§7 World KB canvas structured patch/read routes extending the V1.71/V1.72 patch-route convention; additive World KB DTOs; per-row OCC with `expected_version`/`version`), V1.72 amendment (§7 outline/timeline structured patch routes extending the V1.71 patch-route convention; additive outline DTOs; `@42ch/nexus-contracts` 0.7.0 → 0.8.0 by default), V1.71 amendment (§7 structured patch-route convention for canvas-like surfaces; Strategy β patch routes; `@42ch/nexus-contracts` 0.6.0 → 0.7.0 by default), V1.67 amendment (§3.2 casing ratification + §4 `items` enforcement + §5 sort-param contract; `@42ch/nexus-contracts` 0.5.0 → 0.6.0), V1.64 cursor/error/`items` conventions + V1.65 chapter-content file-backed route rules. |
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

### 6.2 Outline-prose `set.content` persistence is atomic file write + DB metadata update (V1.75)

The V1.65 `PUT /v1/local/works/{work_id}/chapters/{n}/outline` whole-document
write route is **removed in V1.75** (canvas-pivot). Outline prose writes now go
through the canvas patch route
`POST /v1/local/works/{work_id}/chapters/{chapter_id}/patch` with `set.content`
+ `base_revision` (`outline_revision` CAS) — see §7 (V1.75 amendment) and
[canvas-strategy-surface.md](canvas-strategy-surface.md) §3.5. The atomic-write
invariants that this section historically attached to the removed PUT are still
normative; they are re-anchored to the PATCH content path below and enforced by
`crates/nexus-daemon-runtime/src/api/handlers/outline.rs::apply_chapter_patch`
(the `atomic_write_outline` call: sibling temp + rename + file fsync + directory
fsync, run while the caller holds a `RuntimeLockGuard`).

When `set.content` is present on an `outline.patch_chapter` request, the handler
MUST:

1. Load the target `outline_path` from `work_chapters` after Work ownership is
   verified. If absent, initialize the canonical seed path
   (`update_outline_path`) and seed it in the same finalization path as the
   write metadata update.
2. Resolve that path under the active workspace root and apply the path guard in
   §6.5 before creating directories or writing bytes.
3. Write `content` to a sibling temp file, flush/sync, then atomically rename it
   over the final outline file (`atomic_write_outline`: temp + rename + file
   fsync + directory fsync), mirroring the reconcile atomic write pattern in
   `work_chapters::sync_frontmatter_status`.
4. Persist `work_chapters.outline_path` and `updated_at`, and bump
   `frontmatter.outline_revision`, in the same transactional finalization path
   as the file rename so subsequent reads return the new revision. Failed DB
   update or failed rename must clean up the temp file where possible and must
   not report success.
5. Hold the per-Work `RuntimeLockGuard` across the validate → DB persist →
   frontmatter mutate → outline-path seed/write sequence, releasing it on every
   exit path (success and error). Body-ownership invariant: handlers MUST NOT
   mutate `body_path` or body markdown files (see §6.4).
6. Return the committed patch response (`OutlinePatchResponse` with
   `new_revision`), not a speculative echo.

A successful outline-prose patch MUST NOT automatically change chapter `status`.
Status is author-controlled through the explicit chapter structure route /
`outline.patch_chapter` metadata fields.

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
`host_tool_handlers.rs` for body reads and applies it to outline writes (V1.75:
the `outline.patch_chapter set.content` path — see §6.2).

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

## 7. Structured patch routes for canvas-like surfaces (V1.71)

V1.71 promotes the Strategy canvas write-boundary from paper contract to Local API convention. New node/edge edit surfaces that mutate graph-like or structured domain documents SHOULD use an explicit patch route rather than raw file PUTs or broad resource updates:

```text
POST /v1/local/{surface}/{id}/{sub}/patch
```

Examples:

| Surface | Route | Request DTO | Response DTO |
| --- | --- | --- | --- |
| Strategy state | `POST /v1/local/strategies/{strategy_id}/states/{state_id}/patch` | `StrategyPatchStateRequest` | `StrategyPatchResponse` |
| Strategy transition | `POST /v1/local/strategies/{strategy_id}/transitions/patch` | `StrategyPatchTransitionRequest` | `StrategyPatchResponse` |
| Strategy prompt template | `POST /v1/local/strategies/{strategy_id}/states/{state_id}/prompt/patch` | `StrategyPatchPromptTemplateRequest` | `StrategyPatchResponse` |

> **V1.75 amendment — outline `content` field.** `outline.patch_chapter` supports node-scoped chapter edits. In addition to metadata fields (`title`, `slug`, `planned_word_count`, `actual_word_count`, `volume`, `status`), V1.75 adds optional `set.content` for chapter outline prose. `content` is a chapter-outline markdown string persisted to the chapter row's `outline_path` file under the same `base_revision` / `new_revision` `outline_revision` CAS used by outline structure and timeline patches. `content` is not a body editor: handlers MUST NOT mutate `body_path` or body markdown files, and conflict handling remains the standard 409 `OutlineConflictError` with refetch/reapply UX. The V1.65 `PUT /chapters/{n}/outline` whole-document write route is removed in the same bump; the canvas patch route is now the sole outline write path.

### 7.1 Request semantics

Patch request DTOs MUST include:

| Field | Rule |
| --- | --- |
| Resource identifiers | Path identifiers are authoritative. If the body repeats them, the daemon MUST reject mismatches with the canonical error envelope. |
| `base_revision` | Required domain graph/document revision observed by the client during the last canonical read. |
| Patch payload | Minimal, domain-specific mutation shape (`set`, `replace`, `template_patch`, transition target/condition, etc.). Do not accept raw YAML/Markdown/file bytes unless the owning domain spec explicitly authorizes a file-backed route. |

The patch route owns a **single structured operation** at node/edge/subresource granularity. Batch operations require an explicit batch DTO and validation contract; clients must not smuggle multiple unrelated edits through an untyped blob.

### 7.2 Success response envelope

Successful patch responses SHOULD use a domain response DTO with this shape:

```json
{
  "new_revision": 43,
  "validation_summary": {
    "errors": [],
    "warnings": []
  },
  "side_effects": []
}
```

Rules:

1. `new_revision` is the committed revision after the daemon has persisted the patch.
2. `validation_summary` mirrors the domain validator's structured diagnostics and is present even when empty if the domain exposes validation in the UI.
3. `side_effects` is optional and must list only daemon-owned derived updates (for example, normalized labels or rebuilt projection metadata), not speculative client actions.
4. The response is the committed result or commit metadata, not a speculative echo of the request.

### 7.3 Error envelope and conflict semantics

Patch routes MUST use the canonical error envelope from §3.

| HTTP status | Use | Details rules |
| --- | --- | --- |
| `400` | Malformed JSON/body/path mismatch | Stable `<surface>_patch_invalid`-style code. |
| `404` | Target resource or subresource not found | Include the missing id(s) in `details`. |
| `409` | Revision conflict | Include `current_revision`, the target id, `conflicting_path` or equivalent structured locator, and a recovery hint. |
| `422` | Domain validation failure | Include structured validation paths so the UI can focus the failing node/field. |
| `500` | Unexpected daemon failure | Do not leak raw stack traces; include `request_id` when middleware supplies it. |

Revision conflicts are **pre-write** failures: if `base_revision` does not equal the current canonical revision, the handler MUST return 409 before mutating files or DB rows. Validation failures are also non-mutating and MUST NOT increment the revision.

### 7.4 Revision storage and future reuse

The owning domain chooses the revision storage location, but it MUST name a single source of truth and expose the current revision on canonical reads. V1.71 Strategy uses a `revision:` key in the preset YAML header; existing presets without the key read as revision `0` and write `revision: 1` on the first accepted patch.

V1.72 Outline+Timeline canvas surfaces reuse this convention with an outline markdown frontmatter `outline_revision:` key (mirroring the V1.71 preset YAML `revision:` choice): existing outlines without the key read as revision `0` and write `1` on the first accepted patch. The owning domain chooses the revision storage location, but it MUST name a single source of truth and expose the current revision on canonical reads; a future DB-backed revisions table is deferred until audit history, multi-device sync, or collaborative edits require it, and would backfill from frontmatter rather than become a second source of truth.

V1.72 adds 3 outline/timeline patch routes following the V1.71 pattern:

1. `POST /v1/local/works/{work_id}/outline/patch` with `OutlinePatchStructureRequest` (operations: `move_chapter`, `link_event`, `attach_to_volume`) → `OutlinePatchResponse`.
2. `POST /v1/local/works/{work_id}/chapters/{chapter_id}/patch` with `OutlinePatchChapterRequest` (fields: title, slug, wc, volume binding, status) → `OutlinePatchResponse`.
3. `POST /v1/local/works/{work_id}/timeline/patch` with `TimelinePatchEventRequest` (operations: `add_event`, `remove_event`, `attach_event_to_chapter`, `link_foreshadow`) → `OutlinePatchResponse`.

Error envelope (mirrors V1.71 Strategy convention): `OutlineConflictError` (409) extends canonical `ErrorResponse.details` with `current_revision`, `node_id`, `conflicting_path`, recovery hint; `OutlineValidationError` (422) carries structured `validation_summary` mirroring V1.71's `StrategyValidationError`.

V1.73 World KB canvas surface reuses this convention with per-row OCC rather than a single graph/document revision:

1. Client reads a canonical graph/candidate projection with row `version` values.
2. Client submits a node/subresource patch or candidate promotion with `expected_version`.
3. Daemon validates path identifiers, row identifiers, entity-scope-model §5.5 promotion-state invariants, merge/adopt/reject rules, and entity patch payloads.
4. Daemon persists atomically and returns the updated row `version`, or rejects stale writes with 409 and structured recovery data.

This convention does **not** authorize direct browser/Tauri webview writes to raw files. File-backed routes remain domain-specific and must be separately specified like the V1.65 chapter-content routes in §6.

### 7.5 World KB canvas β routes (V1.73)

V1.73 adds the World KB entities + candidates surface to the structured canvas convention. It intentionally ships **2 mutating patch routes** plus **2 read projection routes**; typed relationship CRUD remains `tbd-v1.74-world-kb-relationships`.

| Use | Route | Request DTO | Response DTO |
| --- | --- | --- | --- |
| Patch a World KB entity row | `POST /v1/local/worlds/{world_id}/kb/patch-entity` | `WorldKbPatchEntityRequest` | `WorldKbPatchEntityResponse` |
| Promote a pending candidate | `POST /v1/local/worlds/{world_id}/kb/promote-candidate` | `WorldKbPromoteCandidateRequest` | `WorldKbPromoteCandidateResponse` |
| Read World KB graph projection | `GET /v1/local/worlds/{world_id}/kb/graph` | — | `WorldKbGraphResponse` |
| Read pending candidates | `GET /v1/local/worlds/{world_id}/kb/candidates` | query params as endpoint-defined | `WorldKbCandidatesResponse` |

DTO naming follows the generated filename convention from `schemas/local-api/canvas/world-kb/world-kb-*.schema.json`: generated symbols use the `WorldKb...` entity-prefix form (`WorldKbPatchEntityRequest`, `WorldKbPromoteCandidateRequest`, etc.) even where a schema `title` string uses a verb-prefix phrase. This matches the V1.71/V1.72 generated-contract convention: filenames govern generated public names.

World KB request semantics:

| Field / rule | Requirement |
| --- | --- |
| Path-authoritative `world_id` | The `world_id` path segment is authoritative for World ownership and workspace scoping. Request bodies MUST NOT override it; any repeated/mismatched identifier is a 400 path/body mismatch under the canonical error envelope. |
| Row identifiers | `WorldKbPatchEntityRequest.entity_id` identifies the KeyBlock row under the path `world_id`; `WorldKbPromoteCandidateRequest.job_id` + `candidate_id` identify the pending extraction job/candidate row under the path `world_id`. |
| `expected_version` | Required on both mutating requests. It is the row version observed on the last canonical read: `kb_key_blocks.revision` for entity patches and `kb_extract_jobs.version` for candidate promotion (NULL/absent normalized to `0` where applicable). |
| Response `version` | Mutating success responses return the committed row `version` after persistence. Clients MUST update their local projection from the response or refetch before issuing the next patch. |
| Patch payload | `WorldKbPatchEntityRequest.patch` edits entity fields such as title/body/aliases/block_type. `WorldKbPromoteCandidateRequest.action` is `adopt` / `reject` / `merge`; `merge` requires `merge_target_id`; optional `patch` may refine fields during adoption. |

Conflict and validation errors:

| HTTP status | DTO | Rule |
| --- | --- | --- |
| `409` | `WorldKbConflictError` | Returned before mutation when `expected_version` is stale. Details include `current_version`, `entity_id`, `conflicting_path`, and `recovery_hint`. |
| `422` | `WorldKbValidationError` | Returned for domain-rule failures such as invalid promotion action/target, invalid merge target, invalid entity patch, or entity-scope lifecycle violations. Details carry `validation_summary.errors[]` / `warnings[]`. |

V1.73 graph reads expose source-anchor provenance edges and reserve `relationships` as an empty array until the V1.74 relationship surface. Relationship editing must not be tunneled through `patch-entity` or `promote-candidate` blobs.

### 7.6 World KB relationship patch route (V1.74)

V1.74 extends the World KB canvas β routes with a first-class relationship patch route and with populated relationship projections on the existing graph read.

| Use | Route | Request DTO | Response DTO |
| --- | --- | --- | --- |
| Patch a World KB relationship row | `POST /v1/local/worlds/{world_id}/kb/patch-relationship` | `WorldKbPatchRelationshipRequest` | `WorldKbPatchRelationshipResponse` |
| Read World KB graph projection | `GET /v1/local/worlds/{world_id}/kb/graph?include_suggested=true` | — | `WorldKbGraphResponse` with `relationships: WorldKbRelationshipProjection[]` |

DTO naming follows the generated filename convention from `schemas/local-api/canvas/world-kb/world-kb-*.schema.json`: generated public symbols use the `WorldKb...` entity-prefix form (`WorldKbPatchRelationshipRequest`, `WorldKbPatchRelationshipResponse`, `WorldKbRelationshipInput`, `WorldKbRelationshipProjection`, `WorldKbRelationshipKind`). Schema `title` text may use a verb-prefix phrase for human readability, but public generated symbols are filename-derived.

**V1.76 `needs_review` + `source` (extraction suggestions):** `WorldKbRelationshipProjection`
exposes `needs_review` (boolean) and `source` (`manual` | `extraction`). `source`
is read-only provenance — `manual` marks author-created rows, `extraction` marks
`nexus.llm.extract`-proposed suggestions. `needs_review` is the lightweight
curation gate: `GET .../kb/graph` defaults to excluding `needs_review = 1` rows
(confirmed graph); `?include_suggested=true` surfaces both confirmed and
suggested relationships and preserves the symmetric-reverse projection rule.
Promotion is clearing `needs_review` through the existing patch-relationship
`update` action (the `relationship` payload carries an optional `needs_review`
field; `false` confirms the suggestion). Omitting `needs_review` on update
preserves the existing flag so a routine edit does not accidentally confirm a
suggestion.

Relationship request semantics:

| Field / rule | Requirement |
| --- | --- |
| Path-authoritative `world_id` | The path segment is authoritative for World ownership and workspace scoping. Request bodies MUST NOT override it; source, target, and anchors must resolve within this World. |
| `action` | Required discriminator: `add`, `update`, or `remove`. |
| `relationship_id` | Server-assigned for `add`; required for `update` and `remove`. |
| `expected_version` | Required for `update` and `remove`; omitted or `0` for `add`. It is the row version observed on the last canonical read and compares against `kb_relationships.revision`. |
| Response `version` | Mutating success responses return the committed row version after persistence. Clients MUST update from the response or refetch before issuing another patch. |
| `relationship` payload | Required for `add` and `update`, omitted for `remove`. Contains `source_entity_id`, `target_entity_id`, `relation_type`, optional `custom_label`, `symmetric`, optional `confidence`, optional `source_anchor_ids`, optional `metadata`, and optional `needs_review` (V1.76: `false` promotes/confirm a suggestion; omit to preserve). |

Conflict and validation errors:

| HTTP status | DTO | Rule |
| --- | --- | --- |
| `409` | `WorldKbConflictError` | Returned before mutation when `expected_version` is stale. Details include `current_version`, `relationship_id`, `conflicting_path`, and `recovery_hint`. |
| `422` | `WorldKbValidationError` | Returned for domain-rule failures such as self-loop, invalid relation type/custom label, cross-World source/target, invalid source anchor ids, out-of-range confidence, or missing relationship payload for add/update. Details carry `validation_summary.errors[]` / `warnings[]`. |

Graph read projection rule: `kb_relationships` stores one directed row. When `symmetric=true`, the graph response emits both the stored direction and a derived reverse projection with the same `relationship_id` and `projection_direction = "symmetric_reverse"`. The reverse projection is read-side only; implementations MUST NOT create a second storage row for the reverse edge.

---

## 8. Handler/schema drift closure

Handlers that serve schema-promoted Local API routes MUST emit `generated::local_api::*` response shapes or structurally equivalent types verified by `schema_drift_detection.rs` in `CheckMode::Strict`.

Future endpoint acceptance requires:

1. JSON Schema under `schemas/local-api/<resource>/` (or `local-api/common/` for shared envelopes).
2. Codegen output committed for Rust and TypeScript.
3. Handler response body aligned to generated Rust DTOs.
4. Strict drift detection coverage.
5. README inventory updated under the affected schema subtree.

---

## 9. Evidence and V1.64 decisions

The V1.63 Local API surface audit identified:

- F-P1: Works uses offset/limit while peers use cursor pagination.
- F-P2: Findings list endpoint lacks a response schema.
- F-P3: List array keys differ across resources.
- F-E1: No standardized error envelope.
- F-F1: Sort parameters are not standardized.

V1.64 closes F-P1, F-P2, and F-E1 for the Web UI data-layer baseline, while documenting F-P3 and F-F1 as future conventions with adapter coverage for MVP.

---

## 10. Local daemon port discovery (V1.66 desktop shell)

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
