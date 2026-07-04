---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-04-v1.89-closure"
verdict: "Approve"
generated_at: "2026-07-04"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (focus: input validation, SQL injection prevention, permission/ownership checks, FK/CASCADE behavior, cross-creator leakage, frontend XSS prevention for user content, migration safety)
- Report Timestamp: 2026-07-04

## Scope
- plan_id: 2026-07-04-v1.89-closure
- Review range / Diff basis: merge-base: e6f92049e9d725520cca529996edc89db5e82a96 → tip: iteration/v1.89 (equivalent to `git diff e6f92049...iteration/v1.89`)
- Working branch (verified): iteration/v1.89
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 67 changed files (primary: crates/nexus-local-db/src/reading.rs, crates/nexus-daemon-runtime/src/api/handlers/reading.rs, crates/nexus-local-db/migrations/202607040002_reading_progress_and_annotations.sql, schemas/local-api/reading/*.schema.json + generated types, apps/web/src/components/reading/*, apps/web/src/pages/chapter-page.tsx, apps/web/src/api/queries.ts, crates/nexus-contracts/tests/schema_drift_detection.rs, .sqlx/ cache entries)
- Commit range: e6f92049...451b9024 (P-1 Prepare + P0 backend + P1 frontend merged to integration branch)
- Tools run:
  - `cargo clippy --all -- -D warnings` (clean)
  - `cargo test -p nexus-daemon-runtime -p nexus-local-db -p nexus-contracts` (all relevant tests pass)
  - `pnpm run validate-schemas` (194/194 valid)
  - `pnpm --filter web run typecheck` (clean)
  - `pnpm --filter web run test` (403 passed)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S1 (Low, data hygiene)**: `selected_text` and `note` have no server-side max length bound in DAO or schema (only `minLength:1` on `selected_text` in the annotation schema). For a local-first creator-controlled surface this is acceptable, but consider adding a generous server cap (e.g. 64 KiB for note, 4 KiB for selected_text snapshot) in a future hardening pass to bound storage and rendering cost. (Source: schemas + DAO + handler; no current abuse vector because input originates from the same authenticated creator.)
- **S2 (Low, UX vs security boundary)**: Offset drift detection lives only in the UI (`HighlightLayer`). The server correctly stores whatever offsets the client sends at creation time. This is by design per the spec (drift notice, no auto-reconciliation in MVP). No integrity violation, but worth a one-line comment in the DAO `create_annotation` doc that "offsets are opaque snapshots and may become invalid after body edits."
- **S3 (Info)**: Two new `.sqlx/` cache files were added for the delete queries. This is the correct pattern and matches the crate's documented convention. No action required.

## Source Trace
- Finding ID: (N/A — no blocking findings)
- Source Type: manual code review + verification commands + static analysis (grep for injection sinks, ownership checks, dangerouslySetInnerHTML, raw SQL construction)
- Source Reference: crates/nexus-local-db/src/reading.rs (parameterized queries + explicit creator_id in every path), crates/nexus-daemon-runtime/src/api/handlers/reading.rs (require_active_scope + verify_work_ownership + load_annotation_for_creator), migration file (CREATE TABLE IF NOT EXISTS + CHECK + ON DELETE CASCADE), frontend components (plain JSX text interpolation only; ReactMarkdown for body), schema_drift_detection.rs (all reading schemas registered), test matrix (reading_api.rs + DAO unit tests + web component tests)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 (all non-blocking) |

**Verdict**: Approve

## Detailed Security & Correctness Review

### Backend (Rust DAO + Handlers)
- **Input validation**: Progress clamped 0–10000 (both Rust + DB CHECK). Offsets validated `end_offset > start_offset`. Color whitelisted to the four allowed values. All validated before any DB write.
- **SQL injection**: All queries use sqlx bind parameters (`?1`, `?2`, …) or `sqlx::query!` compile-time macros. No string concatenation into SQL. The two new delete statements have committed `.sqlx/` metadata.
- **Authorization / cross-creator leakage**:
  - Every handler calls `require_active_scope()` (extracts `creator_id` from active workspace).
  - Every read/write path then calls `verify_work_ownership()` (confirms the work belongs to that creator).
  - For annotation mutation (`patch`, `delete`): `load_annotation_for_creator` fetches by `annotation_id` then explicitly checks `row.creator_id != creator_id` and returns `Forbidden`.
  - List/get paths filter by `creator_id` inside the DAO query itself.
- **FK / CASCADE**: Migration declares `FOREIGN KEY (work_id) REFERENCES works(work_id) ON DELETE CASCADE` on both `reading_progress` and `reading_annotations`. Correct for local data hygiene.
- **Idempotency / migration safety**: `CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`. Progress upsert uses `ON CONFLICT(creator_id, work_id, chapter)`. Compatible with V1.88 DBs (additive tables only).
- **No privilege escalation surface**: No path allows one creator to affect another creator's rows.

### Frontend (XSS & Content Safety)
- User-controlled content (`selected_text`, `note`) is rendered via plain JSX text interpolation inside `<blockquote>` / `<p>`. React automatically escapes.
- No `dangerouslySetInnerHTML`, no `innerHTML` assignment, no `v-html` equivalent in the reading components (only test harnesses use `innerHTML` for DOM setup).
- Chapter body is rendered through `ReactMarkdown` + `remark-gfm` (sanitizing markdown renderer). Frontmatter is stripped server-side before the client sees it.
- Annotation color is taken from a strict client-side enum that matches the server whitelist; no raw class or style injection.

### Wire Contracts & Schema Drift
- All seven new reading schemas are registered in `schema_drift_detection.rs` under `Subset` mode (appropriate for local-API DTOs).
- `pnpm run validate-schemas` passed cleanly.
- Generated Rust + TypeScript types are present and consistent with the schemas.

### Test & CI Evidence
- All Rust unit + integration tests for the new modules pass (including ownership negative cases, validation errors, CRUD round-trips).
- Web component tests for `highlight-layer`, `annotation-inspector`, `chapter-page`, etc. pass.
- Full workspace `cargo clippy --all -D warnings`, typecheck, and schema validation gates are green.

## Conclusion
The V1.89 deeper manuscript reading slice (progress + character-offset annotations) implements the required security and data-consistency controls correctly:
- Strong creator-scoped ownership on every path.
- Parameterized SQL with no injection surface.
- Safe (auto-escaped) rendering of user-controlled text.
- Idempotent, backwards-compatible migration with proper CASCADE semantics.

No Critical or Warning findings. The three Suggestions are low-impact observations that do not block the merge.

**Verdict**: Approve
