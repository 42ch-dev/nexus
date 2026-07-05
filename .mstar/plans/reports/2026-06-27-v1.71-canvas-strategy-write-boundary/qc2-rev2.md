---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-27-v1.71-canvas-strategy-write-boundary"
verdict: "Approve"
generated_at: "2026-06-28"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-28

## Scope
- plan_id: 2026-06-27-v1.71-canvas-strategy-write-boundary
- Review range / Diff basis: targeted re-review of P0 fix wave (af992f36 "fix(daemon-runtime): serialize strategy patch writes and validate transition conditions" + 58ac43d3 "feat(web): extract conflict modal..." merged via 5ed2ee6c) on `iteration/v1.71`
- Working branch (verified): iteration/v1.71
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: focus on `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs`, `crates/nexus-daemon-runtime/tests/strategy_patch.rs`, `apps/web/src/components/canvas/conflict-modal.tsx` + `.test.tsx`, `apps/web/src/components/canvas/strategy-canvas.tsx` (post-fix diff)
- Commit range (fix wave): 1afdd592..5ed2ee6c (plus prior integration)
- Tools run: `cargo +nightly-2026-06-26 fmt --all --check` (clean), `cargo clippy --workspace -- -D warnings` (clean), `cargo test -p nexus-daemon-runtime --test strategy_patch` (5/5 passed)

## Revalidation

This is a **targeted re-review (qc2)** after the P0 Strategy β fix wave. Original qc2 (on 2933af37) issued `Request Changes` with 3 Criticals (C1–C3) and 3 Warnings (W1–W3). The fix wave and subsequent web work directly address the blocking items.

### Original 🔴 Critical findings — disposition

- **C1 — Prompt template write is not atomic with YAML revision bump (partial-update hazard)** → **Resolved**
  - Post-fix: `patch_prompt_template_inner` (inside `spawn_blocking`) acquires `StrategyLockGuard` (per-bundle `flock` on `.strategy-lock`), loads YAML + re-checks `base_revision`, then:
    - `backup_existing_file(&canonical_template)`
    - writes body to request-unique `*.tmp.<uuid>` sibling
    - `rename` to final template path
    - `validate_preset_yaml` (on the in-memory YAML + now-visible template)
    - **On validation error**: `rollback_template_write` (restores backup or removes new file) + early return of `strategy_validation_failed` — **no** `write_preset_yaml` and **no** revision bump.
    - **On success**: `write_preset_yaml` (unique tmp + rename + file fsync + parent-dir fsync) + increment revision.
  - All under the exclusive lock; template mutation is staged and rolled back on failure.
  - New test: `patch_prompt_template_rolls_back_on_validation_failure` (creates invalid manifest after template stage → expects validation error + original template content restored, revision unchanged).
  - Directly closes C1 and the related W1.

- **C2 — No cross-request / cross-process OCC or locking for concurrent writers (TOCTOU on base_revision)** → **Resolved**
  - New `StrategyLockGuard` + `acquire_strategy_lock(bundle_dir)` using `nix::fcntl::flock(..., LockExclusive)` on `bundle/.strategy-lock`.
  - All three patch entrypoints (`patch_state`, `patch_transition`, `patch_prompt_template`) wrap their `_inner` work in `tokio::task::spawn_blocking` (keeps blocking lock acquisition off the async runtime).
  - Inside each `_inner` (after lock): `load_user_preset_yaml` → `if req.base_revision != current_revision { return strategy_conflict(...) }` using the freshly-loaded revision.
  - `write_preset_yaml` now uses a request-unique `preset.yaml.tmp.<uuid>` + `atomic_write_with_dir_fsync` (content write + fsync + rename + parent directory fsync for durability).
  - New integration test: `concurrent_patch_state_serializes_and_one_writer_gets_conflict` (two concurrent `patch_state` calls with the same `base_revision: 1`; one succeeds with new_revision=2, the other receives `strategy_conflict` 409; final on-disk revision is 2 and only one label update is present).
  - The advisory lock + post-lock re-check + atomic write closes the TOCTOU window described in original C2.

- **C3 — Conflict modal UX is a stub and does not match locked spec / acceptance criteria** → **Resolved**
  - New file: `apps/web/src/components/canvas/conflict-modal.tsx` (313 LOC) + `conflict-modal.test.tsx`.
  - Implements the full compass §A6 / plan acceptance UX:
    - Headline/body copy: "This state changed while you were editing." + "Nexus updated **{label}** to revision **{current}** while you were editing **{field}**."
    - Field-level "What changed" (canonical) vs "What you were about to do" (draft).
    - Three actions: **Use current** (primary), **Reapply my edit**, **Review side-by-side** (enabled only when `changedFields` indicate non-overlapping paths; disabled for same-field or prompt conflicts).
    - Accessibility: ARIA live region, focus trap on open, Escape handling, return focus to originating control, `prefers-reduced-motion` respected.
  - Integration: `strategy-canvas.tsx` now tracks `lastKnownRevision`, detects `StrategyConflictError`, keeps draft, and renders the full `ConflictModal`.
  - Tests cover open/focus/escape, action callbacks, and disabled state for side-by-side.
  - This was the third blocking Critical; now matches the locked spec.

### Original 🟡 Warning findings — disposition

- **W1 — Template file mutated before validation; no rollback on validation failure** → **Resolved** (see C1 evidence: staging + `rollback_template_write` + early return before YAML bump).

- **W2 — Condition syntax / grammar validation is not re-run on transition patches** → **Resolved**
  - In `patch_transition_inner` (under lock, before structural apply and before `validate_preset_yaml`):
    ```rust
    if let Some(condition) = &req.condition {
        validate_transition_condition(condition)?;
    }
    ```
  - `validate_transition_condition` calls `nexus_orchestration::preset::expr::parse(condition)` and maps parse errors to `BadRequest` with code `strategy_transition_condition_invalid`.
  - New test: `patch_transition_rejects_invalid_condition` (sends a syntactically invalid `condition` with a valid structural target → expects 400 + the specific error code; no revision bump).
  - Grammar/syntax errors are now rejected before any YAML mutation.

- **W3 — Prompt path containment relies on canonicalize + starts_with after a non-atomic assert** → **Partially resolved / acceptably mitigated**
  - The sequence (`assert_template_file_safe` → canonicalize bundle root → join + conditional canonicalize of template → `!starts_with` Forbidden check) now executes while holding the exclusive per-bundle `flock`.
  - The subsequent template staging (temp + rename) and `validate_preset_yaml` also run under the same lock before any YAML revision bump.
  - A symlink-swap attack between the containment check and the write is no longer concurrent with another writer; the lock serializes the entire check → stage → validate → commit window for that bundle.
  - For the local single-author threat model (user-owned `~/.nexus42/`), this is acceptable mitigation. The original `assert_template_file_safe` (rejecting `..`, control chars, etc.) and the canonicalize+starts_with guard remain in place as defence-in-depth.
  - Suggestion for future hardening (non-blocking): after canonicalize, re-assert the final path with `assert_template_file_safe` or an explicit "strictly under bundle" predicate before the rename. Current state is sufficient to close the TOCTOU race described in W3.

### 🟢 Suggestion findings

- **S1 — Add regression test for concurrent writers** → **Addressed**
  - `concurrent_patch_state_serializes_and_one_writer_gets_conflict` exercises exactly this scenario and asserts both the conflict response and the final consistent revision.
- **S2 / S3** (surfacing revision in conflict body more explicitly; documenting single-writer assumption) remain relevant for future polish / docs but are not blocking for V1.71 P0 acceptance.

## Source Trace
- Advisory lock: `strategy.rs:31` (const), `43` (StrategyLockGuard), `63` (acquire_strategy_lock using flock), `563/792/910` (acquired in each *_inner).
- spawn_blocking + re-check: `544` (patch_state), `774` (patch_transition), `891` (patch_prompt_template); re-check at `570/797/915`.
- Atomic write + dir fsync: `424` (atomic_write_with_dir_fsync), `456` (dir fsync), `415` (write_preset_yaml).
- Prompt staging + rollback: `980` (backup), `992` (tmp write), `997` (rename), `1008` (rollback on validation failure).
- Transition condition validation: `808` (call site), `512` (`validate_transition_condition` calling `nexus_orchestration::preset::expr::parse`).
- Conflict modal: new `apps/web/src/components/canvas/conflict-modal.tsx` + test + integration in `strategy-canvas.tsx`.
- Tests: `crates/nexus-daemon-runtime/tests/strategy_patch.rs` (5 new/passing).

## Summary
| Severity (from original qc2) | Disposition |
|------------------------------|-------------|
| 🔴 Critical (3) | All Resolved (C1 atomicity+rollback, C2 locking+recheck, C3 full modal UX) |
| 🟡 Warning (3) | W1/W2 Resolved; W3 mitigated to acceptable (lock serializes the window) |
| 🟢 Suggestion | S1 covered by new concurrent test; S2/S3 non-blocking |

**Verdict**: Approve

**Rationale**: The fix wave (daemon-runtime serialization + validation + web conflict modal) fully closes the three P0 Critical correctness/safety issues identified in the original qc2. Per-bundle advisory `flock`, `spawn_blocking` wrappers, post-lock `base_revision` re-check, atomic staged writes with fsync + rollback, explicit `expr::parse` validation for transition conditions, and a complete accessible conflict modal matching the locked compass spec are now in place. Static checks are clean; the five new integration tests (including the concurrent writer case) all pass. No new Critical or high-risk security/correctness issues were introduced in the reviewed changes. The implementation now meets the P0 acceptance criteria for atomic persistence, no-TOCTOU revision checks, and conflict UX.

## Residual Findings (disposition for this plan)
- Original qc2 proposed: R-V171P0-QC2-C1, R-V171P0-QC2-C2, R-V171P0-QC2-C3 → all three now **Resolved** by the fix wave.
- Recommend: PM to mark the corresponding P0 items closed in `status.json` residual_findings (or confirm any qc1-labeled equivalents that were used as proxies during the wave). No new open P0 residuals from this re-review.

(End of report)
