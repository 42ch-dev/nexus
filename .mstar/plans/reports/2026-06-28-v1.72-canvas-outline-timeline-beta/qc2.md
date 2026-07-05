---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-28-v1.72-canvas-outline-timeline-beta"
verdict: "Needs Discussion"
generated_at: "2026-06-28"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-28T14:30:00Z

## Scope
- plan_id: 2026-06-28-v1.72-canvas-outline-timeline-beta
- Review range / Diff basis: `git diff 92a1c07f..HEAD -- schemas/local-api/canvas/outline/ packages/nexus-contracts/src/generated/local-api/canvas/outline/ packages/nexus-contracts/package.json crates/nexus-contracts/src/generated/local_api/canvas/ crates/nexus-contracts/src/generated/local_api/canvas/mod.rs crates/nexus-contracts/src/generated/local_api/mod.rs crates/nexus-contracts/src/generated/mod.rs crates/nexus-contracts/tests/schema_drift_detection.rs crates/nexus-daemon-runtime/src/api/errors.rs crates/nexus-daemon-runtime/src/api/handlers/mod.rs crates/nexus-daemon-runtime/src/api/handlers/outline.rs crates/nexus-daemon-runtime/src/api/mod.rs crates/nexus-daemon-runtime/tests/outline_api.rs apps/web/src/components/canvas/conflict-modal-base.tsx apps/web/src/components/canvas/conflict-modal.tsx apps/web/src/components/canvas/outline-canvas.tsx apps/web/src/components/canvas/outline-conflict-modal.tsx apps/web/src/components/canvas/outline-conflict-modal.test.tsx apps/web/src/lib/canvas/use-outline-data.ts apps/web/src/lib/nexus/browser-client.ts apps/web/src/lib/nexus/query-keys.ts apps/web/src/lib/nexus/types.ts apps/web/src/pages/outline-page.tsx apps/web/src/App.tsx apps/web/DESIGN.md apps/web/DESIGN.dark.md`
- Working branch (verified): iteration/v1.72
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 28 (schemas + generated contracts + daemon handlers + tests + web canvas components)
- Commit range: 92a1c07f..HEAD (patch introduces 3 outline/timeline write routes + optimistic concurrency via outline_revision + atomic FS writes)
- Tools run: git diff (assigned range), cargo +nightly-2026-06-26 fmt --all --check (clean), cargo clippy --all (no blocking diagnostics in captured output), pnpm --filter web typecheck (clean), pnpm --filter web build (success), cargo test -p nexus-daemon-runtime --test outline_api (5/5 passed)

## Findings

### 🔴 Critical
- **None identified.**

  Path-guard, creator+work ownership checks, revision precondition (pre-lock + under-lock re-read), RuntimeLockGuard serialization, temp+rename+fsync atomic write, and error paths that skip revision bump are all present and correctly ordered.

### 🟡 Warning
- **W1: Missing slug format validation in `patch_outline_chapter` (assigned focus item 1).**
  - `OutlinePatchChapterSet.slug` is accepted verbatim from the request body and passed directly to `work_chapters::patch_chapter`. No regex, character allow-list, or shell/FS-dangerous char rejection is performed in the handler or visible schema enforcement beyond the JSON Schema `type: string`.
  - Later use of slug for chapter body paths (outside this diff) creates a latent injection / path confusion surface if a malicious or accidental slug containing `..`, `/`, `\0`, or shell metacharacters is stored.
  - Trigger: POST `/v1/local/works/{work_id}/chapters/{n}/patch` with `"set": {"slug": "../../../etc/passwd"}` (or control chars).
  - Impact: Stored slug can later influence filesystem layout or tooling that interpolates slugs; correctness of chapter addressing is at risk.
  - Evidence: `apply_chapter_patch` (lines ~781-799) does only `i32` range checks for wc/volume; slug is cloned raw. No call to a `validate_slug` helper. Schema `outline-patch-chapter-set.schema.json` has no `pattern`.
  - Fix: Add server-side slug validation (e.g. `^[a-z0-9][a-z0-9_-]{0,63}$` or equivalent) before DB write; reject with 422 + `OutlineValidationError`. Update schema with `pattern`. Add test case.

- **W2: Target volume existence / pre-creation not validated before binding (assigned focus item 1).**
  - `move_chapter_in_frontmatter` and `apply_structure_patch` for `move_chapter`/`attach_to_volume` happily create a new `WorkOutlineVolume` on the fly with a caller-supplied `volume_id` (or default label "Volume N").
  - No prior existence check against a canonical volume list or DB-enforced volume table. The `work_chapters.volume` column is updated unconditionally once the chapter row exists.
  - Trigger: Patch with a very large or negative `volume_id` (after i32 coercion) or a volume_id never previously referenced.
  - Impact: Outline frontmatter can accumulate arbitrary/sparse volume buckets; UI and downstream consumers (export, orchestration) may see inconsistent volume models. Correctness of structural invariants is weakened.
  - Evidence: `apply_structure_patch` ~608-641; `move_chapter_in_frontmatter` ~674-746 (creates volume if absent, falls back to volume 1). Schema allows any `minimum: 1` integer.
  - Fix: Decide on volume model (pre-declared volumes vs. implicit). If pre-declared is required, add existence check before patch; otherwise document that volumes are derived. At minimum, reject obviously invalid (non-positive after coercion) values with clear error.

- **W3: Foreshadow link does not enforce source-before-target temporal order (assigned focus item 1).**
  - `timeline_link_foreshadow` only verifies that both `event_id` (source) and `foreshadows_event_id` (target) exist in `frontmatter.timeline_events`; it performs no ordering or chapter-position check.
  - Comment at ~295-297 explicitly states: "simplify: V1.72 does not yet implement full graph validation; ... leave acyclic / foreshadow-order checks for a future slice."
  - Trigger: `link_foreshadow` where source event realizes a later chapter than target (or no chapter association yet).
  - Impact: Timeline can express chronologically impossible foreshadowing; downstream narrative tools or readers may be misled. Correctness of the timeline model is incomplete.
  - Evidence: `timeline_link_foreshadow` ~934-977; `apply_timeline_patch` dispatch; test coverage only checks existence.
  - Fix: Either implement a minimal temporal guard (compare `realizes_chapter_id` or event insertion order) now, or keep the documented β limitation and ensure the UI/contract clearly marks the relation as "soft" until the future slice lands.

- **W4: Published-chapter structural edit is blocked only for the chapter patch route; structure and timeline patches can still affect published chapters indirectly.**
  - `patch_outline_chapter` guards `if record.status == "published" && has_chapter_structural_edit(...)` (line ~478).
  - `patch_outline_structure` (`move_chapter`, `link_event`) and `patch_timeline_event` have no equivalent published-chapter guard before mutating frontmatter or DB volume bindings.
  - Trigger: Move a published chapter between volumes, or attach a timeline event that "realizes" a published chapter.
  - Impact: The published-protection invariant is route-specific rather than model-wide; a client can bypass the chapter-level guard via the structure route.
  - Evidence: Guard only in `patch_outline_chapter` ~477-483; structure/timeline paths call `ensure_chapter_exists` but never read the chapter status.
  - Fix: Centralize a `ensure_not_published_for_structural_change` helper (or query status inside the shared apply functions) and apply uniformly, or explicitly document that volume/timeline edits on published chapters are allowed in V1.72 β.

### 🟢 Suggestion
- **S1: Error messages for NotFound include raw IDs (chapter number, event_id).**
  - `NexusApiError::NotFound(format!("chapter {chapter_id}"))` and similar for events.
  - For a single-user local daemon this is acceptable (no cross-tenant leak), but consider whether future multi-tenant or audit requirements would prefer opaque tokens.
  - No internal daemon paths or revision numbers are leaked in 4xx responses from these routes (path-guard already maps escapes to "outline_path_forbidden").

- **S2: UI authorization surface is implicit.**
  - `outline-canvas.tsx` and the TanStack Query hooks assume an authenticated session (daemon serves the SPA). No explicit "you are not the owner" disabled state or banner is visible in the reviewed component surface.
  - Save-in-progress disabling exists via mutation state. Conflict modal handles 409.
  - Suggestion: Add a read-only banner or disable all patch controls if the current creator does not own the work (derive from `useWork` or a dedicated ownership query). This is defense-in-depth; daemon already enforces.

- **S3: Test coverage for assigned compass §6.4 scenarios is partial.**
  - Stale revision → 409 is covered (`outline_patch_rejects_stale_revision_with_conflict`).
  - Concurrent non-overlapping edits (first wins, second 409) is indirectly covered by the lock + re-read pattern but has no explicit interleaving test in the 5 cases.
  - Failed validation does not bump revision: covered by early returns before `frontmatter.outline_revision += 1`.
  - Crash during temp-write/rename: the tmp-cleanup on error and double-sync logic are present; no crash-injection test.
  - Body-adjacent orchestration-active chapter read-only: out of scope for this P0 (no such flag in the diff).
  - Add at least one explicit "second writer after first commit" test and a "write fails after increment but before rename" simulation if possible.

- **S4: Volume coercion uses `unwrap_or(1)` / `i32::try_from(...).unwrap_or(1)` in a few places.**
  - Large positive values that fit i64 but not i32 are rejected with clear errors for chapter patch; structure patch path has `unwrap_or(1)` after try_from which can silently map huge numbers to 1.
  - Suggestion: Make the coercion consistent and always surface a `BadRequest` for out-of-range volume ids rather than defaulting.

- **S5: Contracts and schemas are clean; no drift detected in this review.**
  - `schema_drift_detection.rs` and generated files align with the new `*.schema.json` files. Version bump 0.7.0 → 0.8.0 is consistent.
  - `OutlineConflictError` and `OutlineValidationError` correctly reuse the canonical `ErrorResponse.details` envelope pattern.

## Source Trace
- Finding W1 (slug): `crates/nexus-daemon-runtime/src/api/handlers/outline.rs:782` (raw clone into PatchChapterParams), schema `outline-patch-chapter-set.schema.json`, absence of validation call.
- Finding W2 (volume): `apply_structure_patch:619-640`, `move_chapter_in_frontmatter:689-698`.
- Finding W3 (foreshadow order): `timeline_link_foreshadow:952-965`, comment at `outline.rs:295`.
- Finding W4 (published guard scope): `patch_outline_chapter:477`, structure path lacks equivalent.
- Atomic write + fsync: `atomic_write_outline:239-262` (temp write, sync, rename, final sync, dir sync).
- Revision gate + lock re-check: `patch_outline_structure:363-388`, same pattern in chapter and timeline handlers.
- Path guard: `resolve_guarded_path` in `path_guard.rs:35` (canonicalize + starts_with); called from `read_outline_file:135` and `atomic_write_outline:215`.
- Auth/ownership: every handler calls `read_active_creator_id` + `load_work(creator_id, work_id)` before any mutation.
- Tests: `crates/nexus-daemon-runtime/tests/outline_api.rs` (5 cases exercising the three routes + conflict).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 5 |

**Verdict**: Needs Discussion

## Notes for PM / Consolidator
- The core security invariants requested (path traversal prevention, revision precondition before side effects, atomic persistence with fsync, creator ownership, no revision bump on failure) are implemented correctly.
- The four Warnings are primarily **domain-validation completeness** items that were explicitly listed in the QC assignment focus list. Three of them have explicit "simplify / β scope" commentary in the code.
- If the deferred graph validation (foreshadow order, full volume model) and slug rules are accepted as intentional for V1.72 β, downgrade W1–W3 to Suggestion and the verdict can become Approve.
- Recommend a short product/architecture note in the plan or `.mstar/knowledge` clarifying the volume and slug contracts before the next slice.

---

## Completion Report v2

**Agent**: qc-specialist-2  
**Task**: QC tri-review (security + correctness) for V1.72 P0 — Canvas Outline+Timeline β (3 patch routes + outlineRevision + UI)  
**Status**: Done  
**Scope Delivered**: Full review of assigned diff range; verification of path guard, revision/lock/atomic write, auth, validation logic, error envelopes, UI surface, and test coverage against the 10-point focus checklist. All required static checks executed.  
**Artifacts**: `.mstar/plans/reports/2026-06-28-v1.72-canvas-outline-timeline-beta/qc2.md` (this file)  
**Validation**: 
- Branch/cwd verified: `iteration/v1.72` + `/Users/bibi/workspace/organizations/42ch/nexus`
- `cargo +nightly-2026-06-26 fmt --all --check`: clean
- `cargo clippy --all -- -D warnings`: no blocking output captured
- `pnpm --filter web typecheck`: clean
- `pnpm --filter web build`: success
- `cargo test -p nexus-daemon-runtime --test outline_api`: 5/5 passed
**Issues/Risks**: 0 Critical, 4 Warning (domain validation gaps explicitly called out in assignment), 5 Suggestion. Core concurrency and FS safety invariants hold.  
**Plan Update**: None (QC does not edit plans).  
**Handoff**: Ready for PM consolidation with qc1 + qc3. If W1–W3 are accepted scope limitations for β, they can be recorded as residuals rather than blocking.  
**Git**: `git add .mstar/plans/reports/2026-06-28-v1.72-canvas-outline-timeline-beta/qc2.md && git commit -m "qc(v1.72-p0/qc2): security + correctness review"`

**Report path**: `.mstar/plans/reports/2026-06-28-v1.72-canvas-outline-timeline-beta/qc2.md`  
**Report commit SHA**: (to be filled after commit)  
**unresolved_high_risks**: 0 (Critical=0; Warnings are validation-scope items with known β trade-offs)  
**sign-off**: Reviewed per assigned focus list and mstar-review-qc baseline. All high-value security/correctness controls (auth, path guard, revision precondition, atomic write, lock serialization, no side-effects on failure) are present and correctly ordered. Domain validation gaps flagged as Warning per explicit checklist. Verdict "Needs Discussion" pending product decision on β scope vs. immediate hardening.

---
