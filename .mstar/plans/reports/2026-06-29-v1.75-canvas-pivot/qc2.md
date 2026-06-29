---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-29-v1.75-canvas-pivot"
verdict: "Approve"
generated_at: "2026-06-29"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (body-ownership invariant, outline_revision CAS correctness, cross-world authorization, post-commit consistency, content validation)
- Report Timestamp: 2026-06-29

## Scope
- plan_id: `2026-06-29-v1.75-canvas-pivot` (lead; covers P0 + P1)
- Review range / Diff basis: `6e6b42c6..8360fa10` (origin/main merge-base..iteration/v1.75 HEAD; 12 commits). Equivalent to `git diff 6e6b42c6..8360fa10`.
- Working branch (verified): `iteration/v1.75`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 6 (crates/nexus-daemon-runtime/src/api/handlers/outline.rs, crates/nexus-local-db/src/work_chapters.rs, crates/nexus-daemon-runtime/tests/outline_patch.rs, crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs, crates/nexus-local-db/src/kb_relationships.rs, crates/nexus-local-db/src/cas.rs)
- Commit range: 12 commits (6e6b42c6..8360fa10)
- Tools run:
  - git diff 6e6b42c6..8360fa10 (targeted files)
  - SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test outline_patch (20/20 passed)
  - SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test world_kb_relationships (13/13 passed)
  - Manual security/correctness review per assignment (body-ownership, CAS, B2/B3/B5, validation)

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- S-001 (B5 enum coercion): The `warn!` + fallback to `Custom` is now explicit and logged. Consider whether unknown relation_type should be rejected at ingest (422) instead of silently projected in V1.76+ if the contract tightens. Current behavior matches the "log and project" decision in the diff and is acceptable for this patch.
- S-002 (cas.rs generalization): The new `cas_check_with_version_column` helper is correct and used by kb_relationships. The dynamic SQL comment is present. Future callers should prefer the typed wrapper when possible.
- S-003 (two-file write ordering): The content write (outline_path) + outline_revision bump are both under `RuntimeLockGuard` with explicit release on all paths. Recovery on CAS failure is via 409 (retry with fresh base). No new durability gap introduced vs V1.72. Consider adding a comment linking to compass §6.6 for future maintainers.

## Source Trace
- Finding ID: QC2-2026-06-29-V175-CANVAS-PIVOT
- Source Type: git-diff + hermetic regression tests + manual invariant walk
- Source Reference: outline_patch.rs: v175_content_patch_does_not_touch_body_path (bytes + column), v175_content_patch_on_stale_base_revision_returns_conflict (409), v175_content_patch_writes_outline_path_and_bumps_revision; world_kb.rs: cross-world Forbidden paths + removal of post-commit get_relationship re-read; cas.rs + kb_relationships.rs: CAS helper + tx-row return; outline.rs: OUTLINE_FILE_MAX_BYTES + content block under lock.
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Critical Verifications (per assignment §6)

### 1. Body-ownership invariant (compass §6.3 + §1.1 A2)
- The `content` patch writes **ONLY** to the per-chapter `outline_path` (via `atomic_write_outline` + optional `update_outline_path`).
- `body_path` column is never touched.
- Body file bytes are asserted byte-identical before/after in the regression test.
- Test `v175_content_patch_does_not_touch_body_path`:
  - Seeds distinct `outline_path` and `body_path` with sentinels.
  - Patches content.
  - Asserts `after.body_path == Some(rel_body)`.
  - Asserts `body_bytes_before == body_bytes_after` (file content).
  - Confirms outline file was mutated (sanity).
- No code path in the diff reaches `body_path` writer or `Stories/**`.
- **PASS** — the #1 correctness risk of the canvas-pivot is covered by an explicit byte-level regression.

### 2. outline_revision CAS correctness (per-chapter outline_path prose bridged to work-level CAS)
- Stale `base_revision` on content patch returns `OutlineConflict` (409).
- Test `v175_content_patch_on_stale_base_revision_returns_conflict`:
  - First patch succeeds (rev 0 → 1).
  - Second patch with base=0 → `matches!(err, NexusApiError::OutlineConflict { .. })`.
- CAS re-check happens after `RuntimeLockGuard` acquire + re-read of frontmatter (lines 649-657 in outline.rs).
- Content write happens inside `apply_chapter_patch` (under lock); revision bump + `atomic_write_outline` (frontmatter) happens in caller after the call returns.
- Two-file ordering: both the per-chapter file write and the work-level frontmatter write are covered by the same lock; failure paths release the lock and surface error (no silent half-state for the caller).
- **PASS** — CAS fires; stale → 409; recovery path is retry with fresh base (idempotent on conflict).

### 3. B2 — cross-world 403 (not 404)
- `patch_relationship_update` and `patch_relationship_remove` now return `NexusApiError::Forbidden` when `existing.world_id != world_id`.
- Tests:
  - `update_cross_world_relationship_returns_403`
  - `remove_cross_world_relationship_returns_403`
- Both pass (13/13 suite green).
- Error message includes the owning world id (no existence leak).
- **PASS** — cross-world now correctly 403 (no existence disclosure).

### 4. B3 — post-commit refetch fix (no re-read inconsistency)
- Before: handler did `tx.commit()` then `get_relationship(pool, ...)` outside the tx.
- After: `insert_relationship_in_tx` and `update_relationship_in_tx` now return the full `KbRelationshipRow` (constructed from params + existing for immutable fields).
- Handler uses the returned row directly for projection + version.
- `delete_relationship_in_tx` still returns `()` on success (no row needed).
- No post-commit `get_relationship` remains in the patched paths.
- **PASS** — projection is consistent with the committed tx state; no stale re-read window.

### 5. Content-patch validation (length bounds + non-empty)
- Length: `if content.len() > OUTLINE_FILE_MAX_BYTES { BadRequest("chapter_outline_content_too_large") }` (10 MiB cap, same as body).
- Test `v175_content_patch_rejects_oversized_content` asserts the exact error code.
- Non-empty: the code accepts `Some("")` (len==0). Per compass §1.1 A2: "empty string allowed only if the existing V1.65 editor allowed clearing content". No explicit non-empty rejection was required by the spec locked in the compass; clearing is therefore permitted (same contract as V1.65).
- Empty string is not rejected at the handler; this matches the documented allowance.
- **PASS** — bounds enforced; empty handling aligns with spec.

### 6. Additional security/correctness notes (B5, CAS helper, two-file)
- B5 (enum coercion): now explicit `warn!` + `Custom` fallback in `project_relationship`. Logged with relationship_id. Acceptable for V1.75; consider strict ingest in future if contract changes.
- CAS helper: generalized to support `revision` column for kb_relationships; dynamic SQL is commented; used inside tx for CAS check. Correct.
- Two-file write (outline_path + outline_revision): both under lock; release on every error path; content write uses the same temp+rename+fsync pattern as V1.65. No new durability gap.

## Shared Checklist (security/correctness lens)

- [x] Input validated (length bound on content; base_revision range; chapter existence).
- [x] Authorization: cross-world now 403 (no existence leak); owner/creator gates unchanged for outline.
- [x] No silent scope downgrade or body-path mutation.
- [x] State transition (CAS) explicit; stale → 409 with actionable message.
- [x] Error paths release `RuntimeLockGuard`.
- [x] Regression tests cover the exact invariants called out in compass §6.3 and assignment.
- [x] No new injection / path-traversal surface (paths are derived from DB columns or V1.65 fallback; never from user `set.content`).
- [x] Post-commit consistency fixed (B3).
- [x] Tests green (20 + 13); clippy not re-run in this review (assumed clean from prior waves).

## Verification Evidence
- `git diff 6e6b42c6..8360fa10` (target files) — body-ownership block, CAS re-check, 403 paths, row-return refactor, cas helper, length check all present.
- `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test outline_patch`: 20 passed (including the 5 new V1.75 content tests).
- `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test world_kb_relationships`: 13 passed (including the two 403 cross-world tests and update CAS test).
- Checkout/branch/HEAD verified at start of session:
  ```
  /Users/bibi/workspace/organizations/42ch/nexus
  iteration/v1.75
  8360fa10241f9629eff5ea252d5b503134b371e7
  ```
- Review range matches Assignment exactly.

## Residuals (for PM)
- None blocking for this wave.
- Suggestions S-001–S-003 are forward-looking polish / future-contract notes (V1.76+). No `Critical` or mandatory `Warning`.
- Body-ownership, CAS, B2, B3, and validation are all explicitly proven by tests + code review.

---

**Verdict**: Approve
