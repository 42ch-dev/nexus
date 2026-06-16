---
report_kind: qc-consolidated
plan_id: 2026-06-17-v1.49-narrative-indexes
generated_at: 2026-06-17T20:50:00+08:00
review_range: 3630a4e5..f448b658
working_branch: iteration/v1.49
qc_reports:
  - .mstar/plans/reports/2026-06-17-v1.49-narrative-indexes/qc1.md (qc-specialist, Request Changes — 2 Warnings)
  - .mstar/plans/reports/2026-06-17-v1.49-narrative-indexes/qc2.md (qc-specialist-2, Approve — 0/0/2 Warning + 4 Suggestion [all non-blocking per single-writer/local-first assumptions])
  - .mstar/plans/reports/2026-06-17-v1.49-narrative-indexes/qc3.md (qc-specialist-3, Approve — 0/0/5)
verdict: Request Changes
---

# V1.49 P1 — Narrative Indexes QC Consolidated Report (wave-1, superseded)

## Verdict (wave-1): Request Changes

QC1 (architecture/maintainability) raised 2 Warnings that need to be addressed before merging. QC2 + QC3 approved.

[Rest of wave-1 content preserved; see full file history.]

# V1.49 P1 — Narrative Indexes QC Consolidated Report

## Verdict: Request Changes (one blocker; two Approve)

QC1 (architecture/maintainability) raised 2 Warnings that need to be addressed before merging. QC2 + QC3 approved; their findings (2 + 5 Suggestion) are non-blocking and aligned with the single-writer / local-first assumptions in the plan.

## Findings Roll-up

| Severity | qc1 | qc2 | qc3 | Total | Consolidated |
|----------|-----|-----|-----|-------|--------------|
| 🔴 Critical | 0 | 0 | 0 | 0 | — |
| 🟡 Warning | 2 | 0 | 0 | 2 unique | **W-1 (blocking), W-2 (blocking)** |
| 🟢 Suggestion | 5 | 4 | 5 | 14 unique | non-blocking; V1.50 follow-ups |

Note: qc2 and qc3 reported 0 blocking Warnings; their listed Warnings are framed as non-blocking under documented assumptions.

### W-1 — `ForeshadowingRow.status: String` lacks closed-vocabulary validation (raised by qc1)

- **Location**: `crates/nexus-orchestration/src/narrative_index.rs` (ForeshadowingRow struct)
- **Issue**: The overlay defines a closed vocabulary for status (`planned | buried | paid_off`), but the runtime stores and round-trips the field as `String` without validation. Future code that pattern-matches on status (e.g. for filtering or transitions) will silently accept unknown values.
- **Fix**:
  1. Introduce a typed enum `ForeshadowingStatus { Planned, Buried, PaidOff }` with `FromStr` + `Display` impls (or `serde(rename_all = "snake_case")`).
  2. Update `ForeshadowingRow.status` to the enum type.
  3. `parse_foreshadowing_index` validates the string on parse; reject unknown values with a structured error.
  4. `serialize_foreshadowing_index` uses `Display` for canonical output.
  5. Add tests: `parse_foreshadowing_index_rejects_unknown_status`, `serialize_then_parse_roundtrip_preserves_known_statuses`.
- **Severity**: Warning (not Critical) — runtime currently does not branch on status; the risk is for future code.

### W-2 — `extract_inline_f_declarations` / `promote_outline_to_index` allocates new F### ids for ANY bullet without `F###` prefix (raised by qc1)

- **Location**: `crates/nexus-orchestration/src/narrative_index.rs::extract_inline_f_declarations` (or related outline-parsing function)
- **Issue**: Per qc1, the promotion hook allocates new F### ids for any bullet that lacks the `F###` prefix. Notes, TODOs, prose without explicit `F###` markers silently corrupt the index with spurious ids. This makes the index noisy and dilutes the semantic meaning of "this F### is a real foreshadowing".
- **Fix**:
  1. Make the allocation **explicit**: a bullet must contain the canonical `F###` token (with or without existing id) to be eligible for promotion. Bullets without `F###` are **ignored** (or at most logged at debug level).
  2. Update `extract_inline_f_declarations` to require the `F###` token (existing behavior on lines containing the token, but reject lines without).
  3. Update `promote_outline_to_index` to skip non-declaration bullets entirely.
  4. Add tests: `extract_inline_f_declarations_ignores_bullets_without_f_token`, `promote_outline_to_index_does_not_allocate_for_prose_bullets`.
- **Severity**: Warning (not Critical) — the index is not user-facing beyond the prompt injection; spurious ids are recoverable by manual edit.

### QC2 non-blocking warnings (recorded for completeness)

- qc2 W-1 — parser delimiter collision: edge cases in `parse_foreshadowing_index` when a description contains a literal `|`. Currently tolerates but may produce unexpected column splits. Non-blocking; the implementer's round-trip test covers the normal case.
- qc2 W-2 — deterministic temp file race: temp file path may collide if two promotions fire in the same nanosecond. Local-first single-writer model makes this unrealistic; advisory lock deferred per R-V149P1-01.

PM decision: these are non-blocking under documented assumptions; can be revisited at P-last or V1.50.

### QC3 Suggestions (5) — V1.50 follow-ups

- S-1: promotion hook O(N²) fan-out cost → incremental optimization
- S-2: `read_foreshadowing_summary` no caching (acceptable for MVP)
- S-3: `atomic_write` temp file leak on rename failure (self-healing via deterministic name)
- S-4: no integration test for `promote_foreshadowing_for_schedule` (low-priority test gap)
- S-5: R-V149P1-02 verification on `origin/main` (already verified)

## Residual registration

- **R-V149P1-03 (medium)** — `ForeshadowingRow.status` untyped → typed enum migration (qc1 W-1)
- **R-V149P1-04 (medium)** — `extract_inline_f_declarations` / `promote_outline_to_index` over-allocation → require `F###` token (qc1 W-2)

Both: `decision: fix-in-wave` (current P1 fix wave), `target: V1.49 P1 fix wave`.

## Pre-existing residuals (NOT in this wave)

- **R-V149P1-01** (low, defer P5/P-last) — overlay §3 4-col vs template 5-col schema reconciliation (doc-only).
- **R-V149P1-02** (low, defer V1.50) — pre-existing intermittent flake in `fallback_warn_includes_chapter_field`. **QC3 verified pre-existing on `origin/main @ be27111b` per `.mstar/AGENTS.md` protocol** (2/10 failure rate reproduces on main).
- **R-V149P0-01** (medium, defer V1.50) — CLI `?status=open` gap (P0 follow-up, unrelated to P1).
- **R-V149P0-03** (low, defer V1.50) — pre-existing `cargo clippy --all -- -D warnings` drift. **QC3 reports clean (18.01s)** for the current `iteration/v1.49 @ 990a63b6` — this may indicate the drift was machine-specific to the P0 fix-wave implementer's local toolchain, not a true pre-existing issue. **PM action**: de-prioritize; revisit at P-last if any reviewer observes a regression.

## Next step

PM dispatches **targeted fix wave** to `@fullstack-dev` on a new fix branch from `iteration/v1.49` @ `990a63b6` (current integration HEAD with all 3 QC reports). Fix must:

1. Apply W-1: typed `ForeshadowingStatus` enum + validation + Display.
2. Apply W-2: explicit `F###` token requirement in `extract_inline_f_declarations` + `promote_outline_to_index`.
3. Add regression tests for both Warnings.
4. Re-run CI gates.

After fix wave:
- PM merges fix branch to `iteration/v1.49`.
- QC1 does **targeted re-review** (N=1; only qc1 raised blocking). Updates the **same** `qc1.md` (add `## Revalidation` section, update verdict). qc2 + qc3 stay approved per `mstar-review-qc` default.
- If re-review approves: PM dispatches `@qa-engineer` for the QA pass on the same `Review cwd` + `plan_id` + `Review range` (extended to cover the fix commits).
- After QA passes: PM marks P1 plan `Done` and transitions to P2.

PM notes for tracking:

- New worktree: `.worktrees/v1.49-p1-w1-w2-fix` on `fix/v1.49-p1-w1-w2-typed-and-allocation` (or similar).
- P0 worktrees (`.worktrees/v1.49-findings-lifecycle` + `.worktrees/v1.49-p0-w1-fix`) and P1 worktree (`.worktrees/v1.49-narrative-indexes`) remain for inspection; cleanup deferred.
