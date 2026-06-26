# Body Editor — Specification

| Attribute | Value |
| --- | --- |
| **Status** | Draft — V1.67 design-only; V1.68 implements |
| **Document class** | Draft overlay |
| **Scope** | Local Web UI body editor, per-chapter edit locks, markdown/rich-text round trip, frontmatter/status sync, and conflict policy versus orchestration body writers |
| **Coordinates with** | [chapter-content-local-api.md](chapter-content-local-api.md), [daemon-runtime.md](daemon-runtime.md), [local-api-surface-conventions.md](local-api-surface-conventions.md), `concurrency.md`, `host_tool_handlers.rs` |
| **Authored** | V1.67 Phase 2b (`@architect` + `@product-manager` UX input); PM-integrated |

## 1. Purpose and boundary

V1.68 will add a full-text chapter body editor to the Local Web UI. V1.67 locks the design only. The editor writes the same markdown file currently written by orchestration host tools via `body_path`; therefore V1.68 must add a hard per-chapter edit lock before adding a body write route.

Non-goals for this Draft: collaborative multi-user editing, cloud sync conflict resolution, rich screenplay/export normalization, and changing the existing `work_chapters` table ownership model.

## 2. Existing implementation facts

- Chapter body writes already happen through `host_tool_handlers.rs`: the handler resolves DB-sourced `body_path`, applies a workspace-root path guard, writes a sibling temp file, fsyncs, atomically renames, updates `work_chapters.body_path` / `actual_word_count` / `updated_at`, and reads the row back.
- The existing write path can be gated by an early lock check before temp-file creation; no re-architecture is required.
- V1.51 established a work-level advisory lock at `Works/<work_ref>/.lock` with `flock`, holder metadata, heartbeat, and stale detection. The body editor mirrors the same mental model at chapter granularity.

## 3. Per-chapter edit lock

### 3.1 Lock identity

A body edit lock is scoped to:

```text
(work_id, volume, chapter)
```

The lock file path is:

```text
Works/<work_ref>/.locks/body-v<volume>-ch<chapter>.lock
```

If `work_ref` is unavailable, the implementation may fall back to `work_id` for the directory slug, matching the existing body-path fallback in the host-tool write path.

### 3.2 Holder identity

Lock metadata body:

```text
<pid>:<holder_kind>:<holder_id>:<expires_at_ms>
```

Recommended holder names:

- `web:body-editor:<session_id>`
- `daemon:schedule:<schedule_id>`
- `cli:<command>`

The Local API exposes holder metadata on lock conflict so the UI can show who holds the chapter.

### 3.3 Acquire / heartbeat / release

V1.68 adds Local API routes or equivalent client methods:

- `POST /v1/local/works/{work_id}/chapters/{n}/body-lock`
- `POST /v1/local/works/{work_id}/chapters/{n}/body-lock:heartbeat`
- `DELETE /v1/local/works/{work_id}/chapters/{n}/body-lock`

Acquire is non-blocking. On conflict, return HTTP `423 Locked` with code `chapter_body_locked` and holder details.

The UI acquires the lock when entering edit mode, heartbeats while the editor is open, and releases on save/cancel/navigation/unmount. Release is best-effort; heartbeat expiry handles crashes.

### 3.4 Expiry policy

Mirror V1.51 file-lock timing:

- Heartbeat interval: 30 seconds.
- Stale threshold: 60 seconds after `expires_at_ms`.
- A stale lock may be reclaimed by a new acquirer only after the OS/file-lock check confirms no live holder. If metadata says stale but the lock cannot be acquired, the conflict remains active and the UI reports "holder may be stale".

## 4. Markdown ↔ rich-text round trip

The editor's persistence format is Markdown. Rich text is an editing projection only.

Lossless contract:

1. Preserve YAML frontmatter delimiters and unknown frontmatter fields byte-for-byte unless this spec explicitly updates a known status field.
2. Preserve unsupported markdown nodes as raw markdown blocks; do not silently drop HTML, comments, code fences, tables, footnotes, or unknown directives.
3. Preserve line endings consistently on save; default to `\n` for newly created content.
4. Preserve leading/trailing body whitespace unless the user edits that region.
5. If the editor cannot round-trip a document losslessly, fall back to raw Markdown mode for that chapter and show a non-blocking warning.

V1.68 may choose TipTap with Markdown extensions or a raw textarea-first implementation. The contract is the same: saved bytes must represent the user-visible content plus preserved unsupported nodes.

## 5. Frontmatter/status sync

The body editor reads frontmatter but does not treat frontmatter as decorative metadata.

Sync direction:

- File frontmatter is the human-readable mirror.
- `work_chapters.status`, `actual_word_count`, and `updated_at` are DB query/index fields.
- On body save, the daemon parses frontmatter status if present and reconciles to DB only for recognized chapter statuses: `not_started`, `outlined`, `draft`, `finalized`, `published`.

Trigger:

- Body save triggers frontmatter parse + DB sync.
- Explicit UI status changes still go through `PATCH /chapters/{n}`; the body editor should not silently advance status except when the user explicitly changes frontmatter/status in the editor and confirms the sync.

Conflict rule:

- If frontmatter status differs from DB status and the user did not edit the status field, keep DB status and return a warning.
- If the user edited the status field, validate the transition using chapter status rules before updating DB.
- `published` remains hard-blocked for ordinary edits unless a future publish-retraction design changes policy.

## 6. Conflict policy vs orchestration co-writer

Primary defense: per-chapter lock.

- Web editor owns the chapter while `web:body-editor:*` lock is active.
- Orchestration body writers must check the same lock before writing.
- If the web editor holds the lock, orchestration must skip or queue that chapter body write and report `chapter_body_locked`, not overwrite.
- If orchestration holds the lock, the UI enters read-only mode and shows holder metadata.

Winner policy:

- The active lock holder wins.
- If no lock exists, writes are last-committer-wins, but every writer must acquire the lock immediately before file write finalization.
- The remaining race window is: writer resolves row/path, then another writer acquires before finalization. Recovery is to re-check the lock immediately before temp-file creation and again before rename/DB commit. A failed re-check aborts and leaves the temp file cleaned up.

Recovery:

1. On lock conflict, no file write is attempted.
2. On stale lock, UI offers "reclaim" only after the daemon confirms safe acquisition.
3. On detected file/DB mismatch after crash, the next read returns the DB row plus a `body_consistency_warning`; V1.68 may add a manual reconcile action.

## 7. Host-tool integration

`host_tool_handlers.rs` body writes can be gated without re-architecture:

1. Load Work and chapter row as today.
2. Resolve `work_ref` / `body_path` as today.
3. Acquire/check the per-chapter body lock before creating the temp file.
4. Keep the lock held through temp write, fsync, rename, DB update, final fsync, and commit.
5. Release by dropping the guard.

The lock check is an additive guard around the existing atomic write path, not a replacement for the path guard or DB transaction.
