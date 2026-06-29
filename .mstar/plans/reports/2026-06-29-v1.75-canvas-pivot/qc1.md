---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-29-v1.75-canvas-pivot"
verdict: "Request Changes"
generated_at: "2026-06-29"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3 (MiniMax-cn-coding-plan/MiniMax-M3)
- Review Perspective: Architecture coherence + maintainability (canvas-pivot retire cleanliness, TipTap split cap, CAS dedup, body-ownership invariant, content field contract consistency)
- Report Timestamp: 2026-06-29

## Scope

- plan_id: `2026-06-29-v1.75-canvas-pivot` (lead; consolidated covers P0 canvas-pivot + P1 QC-followup)
- Review range / Diff basis: `6e6b42c6..8360fa10` (origin/main merge-base..iteration/v1.75 HEAD; 12 commits). Equivalent to `git diff 6e6b42c6..8360fa10`.
- Working branch (verified): `iteration/v1.75`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 50 changed (`+1988 / -1246`); primary architecture surface = `crates/nexus-daemon-runtime/src/api/handlers/{outline,chapters,world_kb}.rs`, `crates/nexus-local-db/src/{cas,kb_relationships}.rs`, `apps/web/src/pages/chapter-page.tsx`, `apps/web/src/components/canvas/outline-canvas/{inspectors/chapter-inspector,inspectors/chapter-outline-content-editor,graph-projection}.ts(x)`, `apps/web/src/components/canvas/{outline-conflict-modal,outline-canvas}.tsx`, generated contracts (`packages/nexus-contracts/src/generated/local-api/canvas/outline/OutlinePatchChapterSet.ts` + `crates/nexus-contracts/src/generated/local_api/canvas/outline/outline_patch_chapter_set.rs`), spec promotions (4 specs), harness artifacts (4 plans + status.json + 2 archived residual files)
- Commit range (if not identical to Review range line, explain): identical to Review range line
- Tools run: `cargo clippy -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` (clean), `pnpm --filter web typecheck` (clean), `pnpm --filter nexus-codegen typecheck` (clean); manual grep for stranded references to the removed `PUT /v1/local/works/{work_id}/chapters/{n}/outline` write path (`put_chapter_outline` / `usePutChapterOutline` / `putChapterOutline` / `PutChapterOutlineRequest`)

## Findings

### 🔴 Critical

None. The canvas-pivot retire is clean across the implementation surface; no stranded V1.65 PUT consumers, no dead imports, no regression of the body-ownership invariant, no broken lock-acquire/release ordering (the new content-persistence block in `apply_chapter_patch` runs entirely **after** the caller has acquired `RuntimeLockGuard` and before `frontmatter.outline_revision += 1`, so the existing "release on every exit path" rule remains intact for the new code path).

### 🟡 Warning

#### W-1 — Stale normative text in `chapter-content-local-api.md` §5.4 still describes the removed PUT request/response in detail

- **Finding**: The route table at the top of `.mstar/knowledge/specs/chapter-content-local-api.md` was amended by A7 (line 40 of the spec: the `PUT` row is now struck through and reads "**Removed in V1.75** (canvas-pivot)…"). But §5.4 (lines 200–230) **still** documents the removed endpoint in full: query parameter table, request body sample `{"content": "..."}`, response schema target, and 7 implementation rules (path guard, atomic write ordering, transactional finalization, status-rule, soft-concurrency rule). The reader landing on §5.4 sees a normative description of an endpoint that no longer exists.
- **Fix**: Either (a) collapse §5.4 into a single retired-history stub pointing at the canvas `outline.patch_chapter` + `set.content` path; or (b) move the atomic-write/transactional rules into §5.5 (`PATCH`) under a "outline-prose `set.content` block" subsection, so the durability contract is preserved against the new code path. Option (b) is preferable — the rules are still enforced by `crates/nexus-daemon-runtime/src/api/handlers/outline.rs::apply_chapter_patch` (the `atomic_write_outline` call + temp/rename/fsync/dir-fsync pattern + the body-ownership invariant), so the prose should describe what the new code does, not what the deleted handler did.
- **Source Trace**:
  - Finding ID: F-001
  - Source Type: doc-rule + manual-reasoning
  - Source Reference: `git diff 6e6b42c6..8360fa10 -- .mstar/knowledge/specs/chapter-content-local-api.md` (only 2 lines changed — the route table); manual read of `.mstar/knowledge/specs/chapter-content-local-api.md:200-230`
  - Confidence: High

#### W-2 — Stale normative text in `local-api-surface-conventions.md` §6.2 still describes the removed PUT atomic-write rules

- **Finding**: `.mstar/knowledge/specs/local-api-surface-conventions.md` §6.2 ("Outline PUT is atomic file write + DB metadata update", lines 220–238) still documents the V1.65 PUT route as "the only writable chapter-file route in V1.65" with 5 normative rules covering atomic rename, transactional finalization, status non-bump, etc. This conflicts with the V1.75 amendment note that the same spec now carries under §7 (canvas-pivot amendment: PUT removed; canvas patch route is the sole outline write path). §7 was correctly amended by A7 — §6.2 was not.
- **Fix**: Either delete §6.2 entirely, or rewrite it as a §6.2-bis "outline-prose persistence rules (V1.75)" that points at `outline.patch_chapter` `set.content` and re-derives the atomic-write invariants against the new code path (RuntimeLockGuard acquisition, `outline_path` resolution + `update_outline_path` seeding, the `atomic_write_outline` temp+rename+fsync call, and the body-ownership invariant — all of which are still normative and still need to live somewhere).
- **Source Trace**:
  - Finding ID: F-002
  - Source Type: doc-rule + manual-reasoning
  - Source Reference: `git diff 6e6b42c6..8360fa10 -- .mstar/knowledge/specs/local-api-surface-conventions.md` (2 lines changed — §7 amendment only); manual read of `.mstar/knowledge/specs/local-api-surface-conventions.md:220-238`
  - Confidence: High

### 🟢 Suggestion

#### S-1 — `chapter-inspector.tsx` is now 248 lines; consider extracting `MetaField` + `INPUT_CLASS` to a tiny shared file before the next inspector change pushes it past 250

- **Finding**: The V1.73 split cap (≤250 lines) is now exactly satisfied (248 lines). `MetaField` and `INPUT_CLASS` (lines 236–248) are inspector-local helpers; if a future A-stage adds another metadata field, the file will trip the cap again. Extracting `MetaField` + `INPUT_CLASS` to e.g. `inspectors/inspector-field.tsx` would give ~14 lines of headroom.
- **Fix**: Low priority — not blocking. Either accept the cap as a hard ceiling (then the next change must extract) or extract now while the seam is fresh.
- **Source Trace**:
  - Finding ID: F-003
  - Source Type: manual-reasoning
  - Source Reference: `apps/web/src/components/canvas/outline-canvas/inspectors/chapter-inspector.tsx:236-248`
  - Confidence: Medium

#### S-2 — `apply_chapter_patch` now carries a `#[allow(clippy::too_many_arguments, clippy::too_many_lines)]`; the comment correctly explains why but consider the seam it preserves

- **Finding**: The comment on `apply_chapter_patch` (`crates/nexus-daemon-runtime/src/api/handlers/outline.rs:948-955`) explicitly states the trade-off: "keeping the validate → DB persist → frontmatter mutate → outline-path seed/write sequence inline so the lock-release and body-ownership invariants stay locally auditable". This is the right call for an iteration that ships a body-ownership invariant, but the file's helper list will keep growing (it already carries `update_outline_path` + `atomic_write_outline` + DB persist + frontmatter mutate, all in one function body). A future iteration may want to split the content-persistence block into a `persist_chapter_outline_content` helper while keeping the lock release in the caller.
- **Fix**: Future refactor — not blocking. The current allow + comment is well-justified per the existing pattern in this file (the metadata branch also uses `too_many_arguments`).
- **Source Trace**:
  - Finding ID: F-004
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/outline.rs:948-955` + 1039-1105 (the content persistence block)
  - Confidence: Medium

#### S-3 — `chapter-outline-content-editor.tsx` carries two useEffects with non-obvious coupling (`patchIsPending` → clear `saving` flag; `outline.data` → `setContent` reset)

- **Finding**: The editor has two closely-coupled effects that look ad-hoc:
  - `useEffect` on `[patchIsPending, saveState]` (lines 123–130) — clears the local `saving` flag once the orchestrator mutation settles.
  - `useEffect` on `[editor, outline.data, outline.isFetching, contentVersion]` (lines 110–120) — resets editor content when the read returns new data or the orchestrator bumps `contentVersion`.
  
  Both are intentional and each comment explains the trigger, but the interaction between them (`setSaveState('clean')` from the reset effect, vs. `setSaveState('clean')` from the pending effect) is not pinned down by tests. A future refactor that flips one of them risks racing the other.
- **Fix**: Add a small unit test that asserts `saveState` returns to `clean` after a successful `onPatchChapter` regardless of whether the parent bumped `contentVersion`. The existing `chapter-outline-content-editor.a11y.test.tsx` covers the toolbar contract; a complementary `save-state.test.tsx` would lock the transition logic.
- **Source Trace**:
  - Finding ID: F-005
  - Source Type: manual-reasoning
  - Source Reference: `apps/web/src/components/canvas/outline-canvas/inspectors/chapter-outline-content-editor.tsx:110-130`
  - Confidence: Medium

#### S-4 — Note for `B7` (graph-projection docstring) is an incremental update — the file still describes a "V1.74 read-only behavior" subtly in the surrounding context

- **Finding**: The docstring now correctly describes V1.74 typed relationships + the relationship edges sourced from `WorldKbGraphResponse.relationships`. But the file's behavior around `flattenPages`/candidate projection (unchanged in this iteration) was authored when relationships were deferred — review shows no other V1.73 vs V1.74 confusion in the docstring, just one sentence update. Not blocking.
- **Source Trace**:
  - Finding ID: F-006
  - Source Type: manual-reasoning
  - Source Reference: `apps/web/src/components/canvas/world-kb/graph-projection.ts:1-18`
  - Confidence: Medium

## Source Trace

- Finding ID: F-001 — see W-1 above
- Finding ID: F-002 — see W-2 above
- Finding ID: F-003 — see S-1 above
- Finding ID: F-004 — see S-2 above
- Finding ID: F-005 — see S-3 above
- Finding ID: F-006 — see S-4 above

## Architecture Verifications (passed)

| Check | Outcome | Evidence |
|-------|---------|----------|
| V1.65 PUT route + DTO fully removed (A6) — no orphaned imports/consumers | ✅ Pass | grep finds zero live `put_chapter_outline` / `usePutChapterOutline` / `putChapterOutline` / `PutChapterOutlineRequest` references in `crates/nexus-daemon-runtime/src/**`, `crates/nexus-contracts/src/**`, `packages/nexus-contracts/src/generated/**`, `apps/web/src/**` (the two hits — `apps/web/src/pages/chapter-page.tsx:11` and `apps/web/src/lib/nexus/adapter-contract.test.ts:132` — are comments documenting the removal). Route binding `works_routes()` now registers only `get(handlers::chapters::get_chapter_outline)`. Schema file `schemas/local-api/works/chapters/put-chapter-outline-request.schema.json` removed; schema drift registration dropped. |
| chapter-inspector.tsx ≤250 lines + the extracted TipTap module a clean facade | ✅ Pass | `chapter-inspector.tsx` = 248 lines; the TipTap module (`chapter-outline-content-editor.tsx` = 310 lines) is a single-purpose module with one exported component + 3 small helpers (`getMarkdown`, `SaveStateIndicator`, `EditorToolbar`); no leaking of inspector concerns (no `outline.volumes`, no patch_chapter-mutation plumbing, no chapter-state derives) — the orchestrator pushes `patchIsPending`/`contentVersion` in as props, which keeps the seam one-way. |
| chapter-page.tsx morph clean — no dead `usePutChapterOutline`/TipTap imports | ✅ Pass | 481 → 222 lines. `grep` finds zero live references to `usePutChapterOutline`, `TipTap`, `useEditor`, `EditorContent`, `StarterKit`, `EditorToolbar`, `useChapterOutline`, `SaveStateIndicator`, `SoftConcurrencyBanner`, `ProtectedEditBanner`, `Tabs` (the only `Tabs`/`Markdown` hits are in JSX/comments documenting the removal). Body read-only render (`BodyReadOnly`), header (`ChapterPage` top), `Copy Path`, `PathContextMenu`, and the read queries (`useChapter`, `useChapterBody`) are preserved verbatim. The negative assertion in `chapter-page.test.tsx:174-184` ("does not render any TipTap editor surface") locks this in. |
| B4 CAS dedup genuinely reuses the helper | ✅ Pass | `crates/nexus-local-db/src/cas.rs` now exposes `cas_check_with_version_column` (the canonical, executor-generic + version-column-parameterized helper) and `cas_check` (a 13-line thin wrapper that calls the canonical with `"version"`). `kb_relationships.rs::{update_relationship_in_tx, delete_relationship_in_tx}` both call `cas_check_with_version_column(&mut **tx, …, "revision", …)` — no duplicated re-read logic. |
| `content` field consistent with the outline patch convention + V1.72 patterns | ✅ Pass | The new `content` field on `OutlinePatchChapterSet` is `Option<String>` with `maxLength: 10485760` (10 MiB, mirroring `OUTLINE_FILE_MAX_BYTES` in the daemon). Generated TS + Rust derive identically. The handler validates the cap and returns `BadRequest { code: "chapter_outline_content_too_large" }` (lowercase snake_case, public wire code, per the `NexusApiError` AGENTS rule). The conflict modal label `chapter_outline_content` was added to `OutlineChangedField` + `FIELD_LABELS` so 409 UX reports prose edits accurately. The schema description explicitly states the body-ownership invariant ("MUST NOT mutate body_path"). |
| Body-ownership invariant preserved | ✅ Pass | `crates/nexus-daemon-runtime/src/api/handlers/outline.rs::apply_chapter_patch` content block writes ONLY to `outline_path`; the comment at lines 1039-1051 documents the invariant; `tests/outline_patch.rs::v175_content_patch_does_not_touch_body_path` (line 759) verifies `body_path` column AND body file bytes are byte-identical before/after. |
| RuntimeLockGuard acquire/release ordering preserved (per `crates/nexus-daemon-runtime/AGENTS.md` rule) | ✅ Pass | The new content-persistence block sits **inside** the lock-protected section of `patch_outline_chapter` (between lock acquisition at line 644 and the explicit `lock.release().await` at line 685). Errors inside `apply_chapter_patch` are propagated to the caller's `if let Err(e) = &result` arm, which calls `lock.release().await` before returning — so every exit path still releases the lock. |
| Static checks | ✅ Pass | `cargo clippy -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` clean. `pnpm --filter web typecheck` clean. `pnpm --filter nexus-codegen typecheck` clean. |
| Scope discipline (no scope creep beyond canvas-pivot + 8 residuals) | ✅ Pass | 50 files changed, all falling into: (a) Track A canvas-pivot implementation (A1 wire DTO + A2 daemon persistence + A3 inspector TipTap + A4 parity tests + A5 chapter-page morph + A6 PUT removal + A7 spec promotions + A8 a11y); (b) Track B QC-followup (B1 codegen Eq no-op + B2 cross-world 403 + B3 post-commit refetch + B4 CAS dedup + B5 enum warn + B6 pagination TODO + B7 docstring + B8 test naming); (c) harness artifacts (4 plans + status.json + 2 archived residual files + 1 closure plan). No new surfaces, no mobile, no platform publish, no body editor, no other surfaces touched. |
| Tooling migration: `@42ch/nexus-contracts` 0.10.0 → 0.11.0 | ✅ Pass | `packages/nexus-contracts/package.json` version bumped; additive DTO + V1.65 PUT route/DTO removal both under this bump per the architect lock (no `0.10.1` fallback). |
| `useEditor` deps pinned so re-renders don't re-initialize | ✅ Pass | `chapter-outline-content-editor.tsx:86` — `const editorExtensions = useMemo(() => [StarterKit, Markdown], []);` — the deps array is empty, so the editor doesn't re-init on re-renders. |
| Conflict projection extension | ✅ Pass | `outline-canvas/graph-projection.ts::changedFieldsOf` now pushes `'chapter_outline_content'` when `set.content !== undefined`; the matching `FIELD_LABELS['chapter_outline_content'] = 'Chapter outline content'` in `outline-conflict-modal.tsx` ensures the modal renders the right label. |
| B6: V1.76 pagination TODO comment placed correctly without changing wire contracts | ✅ Pass | `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs::get_graph` (lines 875–881) — a comment block describing the future `limit`/`cursor`/`truncated` shape, with an explicit "Keep V1.75 contract unchanged" note. No code change. |

## Residual Tracking Recommendations

The 2 🟡 Warnings (W-1, W-2) are documentation drift in `Legacy scope` / master specs that the compass A7 mandate explicitly asked to amend. They do not block code merge but they do block a clean `Done` per the residual gate — recommend PM either:
1. Spec hygiene PR (small, surgical, ideal): delete §5.4 of `chapter-content-local-api.md` and §6.2 of `local-api-surface-conventions.md`, folding their normative atomic-write rules into a §6.2-bis / §5.4-bis pointing at the new code path.
2. Defer to a follow-up plan + register `R-V175QC1-DOC-001` (`chapter-content-local-api.md` §5.4 stale) + `R-V175QC1-DOC-002` (`local-api-surface-conventions.md` §6.2 stale) at `severity: warning` in `status.json`.

Per the residual gate in `mstar-review-qc`: unresolved `Critical` or `Warning` → `Request Changes`. Both warnings must be resolved (or formally waived with a documented remediation plan) before approval.

The 4 🟢 Suggestions are non-blocking; PM/QA may register them as `Suggestion`-severity residuals for follow-up if the team wants to track them.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes