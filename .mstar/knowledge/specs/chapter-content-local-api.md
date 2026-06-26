# Chapter Content Local API

| Attribute | Value |
| --- | --- |
| **Status** | Draft — V1.65 Prepare contract for P0 implementation |
| **Document class** | Draft overlay |
| **Scope** | Chapter list/detail, outline read/write, structure PATCH, and body read-only Local API contracts under `/v1/local/works/{work_id}/chapters/*` |
| **Coordinates with** | [local-api-surface-conventions.md](./local-api-surface-conventions.md), [daemon-runtime.md](./daemon-runtime.md), [schemas-directory-layout.md](./schemas-directory-layout.md), [web-ui.md](./web-ui.md), `apps/web/DESIGN.md` |
| **Implementation owner** | V1.65 P0 backend implementer; P2 Web UI consumes only via `NexusClient` |

---

## 1. Purpose and boundary

V1.65 exposes the existing `work_chapters` metadata and file-backed chapter content to the bundled local Web UI so authors can plan and restructure a Work without turning the browser SPA into a manuscript co-writer.

The surface is intentionally split:

- **Structure and outline are writable** in V1.65.
- **Body markdown is read-only** — the AI owns prose writing via orchestration through the host-tool path; there is **no manual body editor** (the body-editor direction was rejected 2026-06-26 — Nexus is an AI-autonomous executor; see [canvas-strategy-surface.md](canvas-strategy-surface.md)). Any future human body interaction is a V1.68 canvas concern (structured/node-granular, no-raw-file-editing), not a per-chapter manual write route.
- All routes stay under the Local API and are consumed through the frontend `NexusClient` interface; no browser-only filesystem assumptions are part of the contract.

## 2. Existing implementation facts this contract builds on

- `work_chapters` is the chapter metadata SSOT: `chapter`, `volume`, `slug`, `planned_word_count`, `actual_word_count`, `status`, `outline_path`, `body_path`, `created_at`, `updated_at`.
- `seed_chapters` initializes `outline_path` as `Works/{work_ref}/Outlines/chapters/chNN-outline.md` and `body_path` as `Works/{work_ref}/Stories/chNN-{slug}.md`.
- `work_chapters::update_paths` and `update_status` update DB metadata with `updated_at`.
- `work_chapters::sync_frontmatter_status` demonstrates the filesystem write pattern P0 must mirror for outline writes: sibling temp file, flush, atomic rename, and best-effort temp cleanup on failure.
- `host_tool_handlers.rs` body read path applies a W-002-style path guard around a DB-sourced `body_path`: resolve inside the workspace root, reject traversal, and fail closed when the resolved path escapes the workspace.

## 3. Endpoint summary

All endpoints use the V1.64 Local API error convention: non-2xx responses are emitted as the daemon error wire envelope with `error` shaped by `schemas/local-api/common/error-response.schema.json`.

| Method | Path | Purpose | Mutates file | Mutates DB |
| --- | --- | --- | --- | --- |
| `GET` | `/v1/local/works/{work_id}/chapters` | Cursor-paginated chapter summaries | No | No |
| `GET` | `/v1/local/works/{work_id}/chapters/{n}` | Chapter detail including paths | No | No |
| `GET` | `/v1/local/works/{work_id}/chapters/{n}/outline` | Read outline markdown | No | No |
| `PUT` | `/v1/local/works/{work_id}/chapters/{n}/outline` | Replace outline markdown atomically | Yes, `outline_path` only | Yes, `outline_path` if initialized/normalized + `updated_at` |
| `PATCH` | `/v1/local/works/{work_id}/chapters/{n}` | Structure metadata update | No | Yes |
| `GET` | `/v1/local/works/{work_id}/chapters/{n}/body` | Read body markdown | No | No |

`{n}` is the chapter number within a volume. V1.65 MUST support `volume` as an optional query parameter on detail/content/patch routes; when omitted it defaults to `1` to preserve existing single-volume behavior.

## 4. Common query and DTO rules

### 4.1 Chapter identity

`ChapterIdentity` is represented inline in request paths and DTOs:

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `work_id` | string | yes | Owning Work ID from the path. |
| `chapter` | integer | yes | Positive chapter number. |
| `volume` | integer | yes in responses | Positive volume number; default `1` when omitted by clients. |

### 4.2 Chapter status enum

V1.65 uses the existing chapter status vocabulary:

```text
not_started | outlined | draft | finalized | published
```

Unknown status values from legacy rows SHOULD be passed through in read DTOs only if already present in DB, but write validation MUST reject values outside the enum.

### 4.3 Chapter summary DTO

`ChapterSummary` is the list-row shape for `GET /chapters`:

| Field | Type | Required | Source / rule |
| --- | --- | --- | --- |
| `work_id` | string | yes | DB row. |
| `chapter` | integer | yes | DB row. |
| `volume` | integer | yes | DB row; default `1` if stored null by older data. |
| `title` | string or null | no | Human title if materialized by P0; otherwise clients may derive display text from `slug`/chapter number. |
| `slug` | string or null | no | DB row. |
| `planned_word_count` | integer | yes | DB row. |
| `actual_word_count` | integer or null | no | DB row. |
| `status` | chapter status | yes | DB row. |
| `outline_path` | string or null | no | Relative path from DB; UI may show/copy only after path guard has allowed reads/writes. |
| `body_path` | string or null | no | Relative path from DB; body remains read-only. |
| `created_at` | string date-time | yes | DB row. |
| `updated_at` | string date-time | yes | DB row. |

If P0 decides not to add a `title` column to `work_chapters` in V1.65, `title` MUST remain optional/null in the DTO and structure PATCH MUST reject title writes with a clear `chapter_title_unsupported` error or store title in the agreed metadata field before exposing the write.

### 4.4 Chapter detail DTO

`ChapterDetail` includes all `ChapterSummary` fields plus content metadata:

| Field | Type | Required | Rule |
| --- | --- | --- | --- |
| `can_edit_outline` | boolean | yes | True for existing chapter rows with an in-bounds outline path or initializable outline path. |
| `can_edit_structure` | boolean | yes | False only if Work/chapter policy blocks all structural edits. |
| `body_read_only` | boolean | yes | Always `true` in V1.65. |
| `protection` | object | yes | See finalized/published policy below. |

Suggested `protection` shape:

```json
{
  "level": "none | confirm_structure_edit | hard_block_delete",
  "reason": "Chapter is finalized; structural edits require confirmation."
}
```

## 5. Endpoint contracts

### 5.1 `GET /v1/local/works/{work_id}/chapters`

Query:

| Parameter | Type | Default | Rule |
| --- | --- | --- | --- |
| `cursor` | string | absent | Opaque cursor; clients MUST NOT parse. |
| `limit` | integer | `50` | Handler-bounded. |
| `status` | chapter status | absent | Optional filter; recommended for P2 table filtering. |

Response schema target: `schemas/local-api/works/chapters/list-chapters-response.schema.json`.

```json
{
  "items": [
    {
      "work_id": "wrk_...",
      "chapter": 1,
      "volume": 1,
      "title": null,
      "slug": "ch01",
      "planned_word_count": 4000,
      "actual_word_count": null,
      "status": "not_started",
      "outline_path": "Works/WRK/Outlines/chapters/ch01-outline.md",
      "body_path": "Works/WRK/Stories/ch01-ch01.md",
      "created_at": "2026-06-25T00:00:00Z",
      "updated_at": "2026-06-25T00:00:00Z"
    }
  ],
  "pagination": {
    "limit": 50,
    "next_cursor": null,
    "has_more": false
  }
}
```

Rules:

- New list endpoint MUST use `items` + `pagination` from day one; do not introduce a `chapters` array key.
- Ordering defaults to `volume ASC, chapter ASC`.
- This endpoint does not read markdown files.

### 5.2 `GET /v1/local/works/{work_id}/chapters/{n}`

Query:

| Parameter | Type | Default | Rule |
| --- | --- | --- | --- |
| `volume` | integer | `1` | Positive volume number. |

Response schema target: `schemas/local-api/works/chapters/chapter-detail.schema.json`.

Returns `ChapterDetail`.

Rules:

- Verifies the Work belongs to the active creator before returning chapter data.
- Returns `work_not_found`/`chapter_not_found` style errors using the shared `ErrorResponse` convention.
- Does not read outline/body content; path strings are metadata only.

### 5.3 `GET /v1/local/works/{work_id}/chapters/{n}/outline`

Query:

| Parameter | Type | Default | Rule |
| --- | --- | --- | --- |
| `volume` | integer | `1` | Positive volume number. |

Response schema target: `schemas/local-api/works/chapters/chapter-outline.schema.json`.

```json
{
  "work_id": "wrk_...",
  "chapter": 1,
  "volume": 1,
  "outline_path": "Works/WRK/Outlines/chapters/ch01-outline.md",
  "content": "# Chapter 1\n\n- Beat...\n",
  "updated_at": "2026-06-25T00:00:00Z"
}
```

Rules:

- `outline_path` MUST be DB-sourced. If absent and P0 chooses to initialize a canonical outline path on read, the DB update must be explicit and documented; otherwise return `chapter_outline_not_found` with remediation.
- Before reading, resolve the path inside the workspace root and apply the W-002 path guard described in §7.
- Missing but in-bounds files return a typed file-read/not-found error; path traversal returns validation failure.

### 5.4 `PUT /v1/local/works/{work_id}/chapters/{n}/outline`

Query:

| Parameter | Type | Default | Rule |
| --- | --- | --- | --- |
| `volume` | integer | `1` | Positive volume number. |

Request schema target: reuse `chapter-outline.schema.json` request side or materialize `put-chapter-outline-request.schema.json` if codegen prefers request/response split.

```json
{
  "content": "# Chapter 1\n\n- Revised beat...\n"
}
```

Response schema target: `schemas/local-api/works/chapters/chapter-outline.schema.json`.

Rules:

1. Verify active creator owns the Work and the chapter row exists.
2. Determine the target path from DB `outline_path`. If missing, initialize the canonical seed path `Works/{work_ref}/Outlines/chapters/chNN-outline.md` and persist it in the same transaction as the write metadata update.
3. Resolve the path under the workspace root; reject traversal or symlink escape with `chapter_outline_path_forbidden` (or shared `workspace_path_forbidden` if the implementation standardizes that code).
4. Create parent directories as needed inside the workspace root.
5. Write `content` to a sibling temp file, flush/sync it, then atomically rename it over the final `outline_path`, mirroring `work_chapters::sync_frontmatter_status` around line 552.
6. Update `work_chapters.outline_path` and `updated_at` in the same DB transaction that guards the finalization of the write. If DB update or rename fails, clean up the temp file and do not report success.
7. Return the final `ChapterOutline` content from the committed file/row state.

Status rule: outline PUT is content-only and MUST NOT automatically bump `status` to `outlined`, even when the chapter is `not_started` or `draft`. Status progression is explicit through `PATCH /chapters/{n}`.

Soft-concurrency rule: no hard per-chapter lock exists in V1.65. Orchestration reads outline at draft-time, so its read is a natural snapshot of whatever is on disk then. PUT should be last-write-wins at the file level, with UI warnings for `draft`/`finalized` chapters supplied by P2.

### 5.5 `PATCH /v1/local/works/{work_id}/chapters/{n}`

Query:

| Parameter | Type | Default | Rule |
| --- | --- | --- | --- |
| `volume` | integer | `1` | Positive current volume. |

Request schema target: `schemas/local-api/works/chapters/patch-chapter-request.schema.json`.

```json
{
  "title": "The Third Layer",
  "slug": "the-third-layer",
  "planned_word_count": 4200,
  "volume": 1,
  "status": "outlined",
  "confirm_structural_edit": false,
  "transition_reason": "Outline reviewed"
}
```

Response: `ChapterDetail`.

Writable fields:

| Field | Type | Rule |
| --- | --- | --- |
| `title` | string or null | Optional in V1.65; only expose if storage exists. |
| `slug` | string | Slug-safe filename segment; changing it does not rename body/outline files in V1.65 unless P0 explicitly implements safe path migration. |
| `planned_word_count` | integer | Positive bounded integer. |
| `volume` | integer | Positive volume assignment; must preserve `(work_id, volume, chapter)` uniqueness. |
| `status` | chapter status | Only allowed automatic progression is `not_started → outlined`. |
| `confirm_structural_edit` | boolean | Required by UI when editing protected `finalized` chapters. |
| `transition_reason` | string | Required for explicit reverse/terminal transitions when P0 exposes them. |

Status rules:

- `not_started → outlined` is allowed through this PATCH.
- `outlined → draft → finalized → published` remain orchestration/review-owned unless a later explicit endpoint/action is designed.
- Reverse transitions (for example `draft → outlined` or `finalized → outlined`) MUST NOT happen implicitly as a side effect of metadata or outline edits. If P0 implements any reverse transition in V1.65, it must require an explicit action field and a reason; otherwise return `chapter_status_transition_invalid`.
- `published` is terminal for V1.65 UI editing; structural edits should be rejected unless a future publish-retraction design exists.

Protection rules:

- Deletion is not part of this V1.65 surface. If any delete route is later added, `finalized` and `published` chapters are hard-blocked.
- Structural edits to `finalized` chapters require `confirm_structural_edit: true`; without it return `chapter_structure_confirmation_required`.
- Structural edits to `published` chapters are hard-blocked in V1.65.
- Edits to `draft` chapters are allowed but should surface a warning in UI because the body may already exist.

### 5.6 `GET /v1/local/works/{work_id}/chapters/{n}/body`

Query:

| Parameter | Type | Default | Rule |
| --- | --- | --- | --- |
| `volume` | integer | `1` | Positive volume number. |

Response schema target: `schemas/local-api/works/chapters/chapter-body.schema.json`.

```json
{
  "work_id": "wrk_...",
  "chapter": 1,
  "volume": 1,
  "body_path": "Works/WRK/Stories/ch01-ch01.md",
  "content": "---\nstatus: draft\n---\n\nBody prose...\n",
  "frontmatter": {
    "status": "draft"
  },
  "read_only": true,
  "updated_at": "2026-06-25T00:00:00Z"
}
```

Rules:

- Body is read-only in V1.65: no PUT/PATCH body route is introduced.
- `body_path` is DB-sourced and path-guarded exactly like outline reads.
- API may return raw markdown content plus parsed frontmatter metadata for UI rendering. If P0 does not parse frontmatter, `frontmatter` may be omitted from schema v1; the UI can parse client-side. The read-only flag remains required.

## 6. Work profile classification decision

`work_profile` addition to `CreateWorkRequest` and `PatchWorkRequest` is classified as **additive optional**.

Rationale:

1. It preserves V1.64 UI behavior where omitted profile values continue to let the daemon apply its default/profile inference.
2. It lets the V1.65 UI select a profile explicitly without forcing every existing consumer to send the field.
3. It avoids a gratuitous breaking schema change even though the repository is pre-1.0 and could tolerate one.
4. It keeps the P0 backend change small: validate the field when present; default when absent.

Schema consequence: add `work_profile` as an optional property to both request schemas; do not add it to `required`. If P0 needs a DB invariant, enforce/default at persistence time rather than making the wire field required.

## 7. Path-guard specification

All outline/body file routes must treat DB paths as untrusted until resolved.

Algorithm:

1. Load `outline_path` or `body_path` from `work_chapters` after verifying active creator ownership of `work_id`.
2. Join the relative DB path to the daemon workspace root.
3. Canonicalize the workspace root.
4. For an existing target, canonicalize the target and require it to start with the canonical workspace root.
5. For a missing target that is expected to be creatable, canonicalize the nearest existing parent or normalize the joined path and require the resulting absolute path to remain under the workspace root. Do not allow `..` traversal to escape before creation.
6. Reject escape with a typed validation error; do not attempt to create parent directories outside the workspace.

This mirrors the W-002 defense-in-depth guard in `host_tool_handlers.rs` around line 2006, adapted for outline writes where the target may not exist yet.

## 8. Tauri-ready frontend boundary

The Web UI must consume this surface through `NexusClient` methods such as:

```ts
listChapters(workId, query)
getChapter(workId, chapter, query)
getChapterOutline(workId, chapter, query)
putChapterOutline(workId, chapter, body, query)
patchChapter(workId, chapter, patch, query)
getChapterBody(workId, chapter, query)
```

The contract deliberately returns markdown strings and relative workspace paths; it does not require browser filesystem access. Browser-only APIs such as clipboard writes are UI affordances around `Copy path`, not transport semantics. V1.66 can therefore implement `TauriClient` with the same method signatures and add native `open with` / `reveal` affordances outside this API.

## 9. Schema placement

P0 should materialize schemas under:

```text
schemas/local-api/works/chapters/
├── README.md
├── chapter-body.schema.json
├── chapter-detail.schema.json
├── chapter-outline.schema.json
├── chapter-summary.schema.json
├── list-chapters-query.schema.json
├── list-chapters-response.schema.json
└── patch-chapter-request.schema.json
```

If codegen or schema-loader constraints favor a flat `schemas/local-api/works/` folder, P0 may flatten the files with a `chapter-` prefix, but the normative layout target for V1.65 is the `works/chapters/` subtree.
