---
report_kind: qc-consolidated
plan_id: "2026-06-08-v1.38-multi-chapter-selection-status"
verdict: "Request Changes — fix F-001 (Critical) and W-1 (Warning); accept others as residuals"
generated_at: "2026-06-08"
qc_wave: "initial"
active_wave_note: "Initial tri-review. Re-review (qc1-rev2) targets QC1 only after fix lands."
---

# QC Consolidated Report — V1.38 P0 Multi-Chapter Selection and Status

## Gate Verdict

**Request Changes** — one Critical and several Warnings raised across the three reviewers. Fix-now vs residual mapping below.

## Reviewers

| Seat | Reviewer | Verdict | Critical | Warning | Suggestion | Report |
|------|----------|---------|----------|---------|------------|--------|
|1 | @qc-specialist | Request Changes |1 |1 |1 | [qc1.md](qc1.md) |
|2 | @qc-specialist-2 | Request Changes |0 |2 |3 | [qc2.md](qc2.md) |
|3 | @qc-specialist-3 | Request Changes |0 |2 |2 | [qc3.md](qc3.md) |

## Scope alignment (verified verbatim)

- `plan_id`: `2026-06-08-v1.38-multi-chapter-selection-status`
- `Review range / Diff basis`: `merge-base(3f72b085, HEAD)..HEAD` on `iteration/v1.38`
- `Working branch (verified)`: `iteration/v1.38`
- `Review cwd (verified)`: `/Users/bibi/workspace/organizations/42ch/nexus`
- All three reports used the same scope; alignment gate passed.

## CI gate evidence (PM-verified)

- `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings` → exit0 (clean)
- `cargo test -p nexus-local-db work_chapters` →17 passed,0 failed
- `cargo test -p nexus-daemon-runtime --test works_api` →28 passed,0 failed
- `cargo test -p nexus-orchestration --tests` →474 lib + integration tests pass
- `cargo test -p nexus42 --tests` →745+ tests pass (lib608 + integration137)

## Findings consolidation

### 🔴 Critical

| ID | Source | Title | Decision |
|----|--------|-------|----------|
| **F-001** | QC1 | `next_chapter()`3-tier selection skips earlier in-progress chapters in favor of later `not_started` rows; contradicts spec §4.5.2 resume/outlined notes and plan AC2/AC3 | **Fix now** |

### 🟡 Warning

| ID | Source | Title | Decision |
|----|--------|-------|----------|
| **F-002** | QC1 | `is_work_completed()` early-returns true on `works.status == 'completed'` before validating all3 §6.1 conditions | **Fix now** (lightweight) |
| **W-1** | QC3 | Migration lacks composite index `(work_id, status, chapter)` for `next_chapter()` query pattern | **Fix now** (one-line migration) |
| **W1** | QC2 | `next_chapter()` has3 sequential SELECTs without transaction; concurrent `creator run continue` could claim same chapter twice | **Residual — defer** (single-user local-first documented; race window narrow) |
| **W2** | QC2 | Plan T9 surface of on-disk missing-file hints only partially delivered in CLI status | **Residual — defer** (DB SSOT preserved; CLI renders DB truth; `reconcile-chapters` covers remediation) |
| **W-2** | QC3 | Write-on-read anti-pattern: `GET /v1/local/works/{id}` mutates `works.status` on completion | **Residual — defer** (documented as lazy promotion in implementation comment; matches V1.36 P4 fix wave auto-promote pattern) |

### 🟢 Suggestion (non-blocking)

| ID | Source | Title | Decision |
|----|--------|-------|----------|
| **F-003** | QC1 | Prompt templates still format paths as `ch0{{chapter}}` (fragile for ch10+) | **Residual — defer to Plan3** (parameterization is Plan3 scope) |
| **S1** | QC2 | Register race + missing-hint as `residual_findings` with severity medium/low | **Action now** — see residual registration below |
| **S2** | QC2 | Add explicit test for `total_planned_chapters=NULL` early-return in `is_work_completed` | **Residual — defer** (doc comment + early-return already correct) |
| **S3** | QC2 | DAO layering note: `next_chapter` and `is_work_completed` are low-level DAOs; creator scoping done by callers | **Note only — no action** |
| **S-1** | QC3 | No cap on `WorkApiDto.chapters` vector size | **Residual — defer** (typical novel scales ≤100 chapters) |
| **S-2** | QC3 | `next_chapter()` could be a single CTE (3 round-trips →1) | **Resolved by F-001 fix** (single `MIN(chapter)` query replaces3-tier) |

## Fix-now scope (per fullstack-dev re-dispatch)

1. **F-001** — Replace `next_chapter()`3-tier selection with a single query that selects the **lowest chapter** whose status is in the active set `{not_started, outlined, draft}`. Specifically:
 - SQL: `SELECT MIN(chapter) FROM work_chapters WHERE work_id = ? AND status IN ('not_started', 'outlined', 'draft')`.
 - Return `Ok(Some(chapter))` if a row exists, else `Ok(None)`.
 - Update `test_next_chapter_resumes_draft` and `test_next_chapter_outlined_not_skipped` to assert the spec-correct semantics: lowest active chapter wins regardless of status priority (i.e. ch2=draft + ch3=not_started → ch2; ch1=outlined + ch2=not_started → ch1).
 - Verify `test_next_chapter_selects_lowest_not_started` still passes (lowest not_started is the active chapter when all earlier chapters are finalized).
2. **F-002** — Narrow `is_work_completed()` early-exit so the `works.status == 'completed'` shortcut does **not** bypass §6.1 conditions when the Work is novel-profile. Approach: keep the early-exit only when the Work's profile is non-novel (V1.36 compatibility); for novel-profile Works, always run the full §6.1 check.
3. **W-1** — Add a new migration `20260608_work_chapters_composite_index.sql` that creates `CREATE INDEX work_chapters_by_work_status ON work_chapters(work_id, status, chapter)`. Run `cargo sqlx prepare --workspace` and commit the regenerated `.sqlx/` metadata.

After fix, dispatch targeted re-review to `@qc-specialist` (QC1) only — covers F-001 + F-002. W-1's migration is small; it can be re-reviewed by QC1 too (same reviewer) or accepted as a no-code-change auto-pass since it's a one-line index addition. PM will judge after fix.

## Residual registration (root `status.json.residual_findings`)

After Plan2 is marked Done, register these open items (severity per machine enum):

| ID | Title | Severity | Source | Owner | Target |
|----|-------|----------|--------|-------|--------|
| R-V138P0-01 | `next_chapter()` selection race window under concurrent `creator run continue` (single-user assumption) | medium | QC2 W1 | @fullstack-dev | V1.38 P1 fix wave or V1.39 |
| R-V138P0-02 | Plan T9 missing-file hint emission in CLI status not visible (DB rows + comment only) | low | QC2 W2 | @fullstack-dev | V1.38 P1 fix wave |
| R-V138P0-03 | Write-on-read anti-pattern in `GET /v1/local/works/{id}` (lazy completion promotion) | medium | QC3 W-2 | @fullstack-dev | V1.39+ hardening plan |
| R-V138P0-04 | `WorkApiDto.chapters` vector size uncapped for unusual `total_planned_chapters` | low | QC3 S-1 | @fullstack-dev | V1.39+ hardening |
| R-V138P0-05 | `is_work_completed` total_planned_chapters=NULL explicit test missing | nit | QC2 S2 | @fullstack-dev | backlog |

F-003 (prompt template formatting) belongs to Plan3 (`novel-writing` parameterization) — will track under that plan's residuals.

## Diff Scope Check (consolidated)

All3 reviewers confirmed no diff hunks touched the explicitly deferred boundaries:

- Auto-chain / DF-53 — NOT touched
- World KB / DF-63 — NOT touched
- Quality loop / DF-64/65/66/67 — NOT touched
- Multi-volume PK migration — NOT touched
- Platform publish — NOT touched
- Multi-work switch — NOT touched
- Selection pool — NOT touched

Scope boundary holds. Implementation is correctly bounded by the V1.38 compass §1.2.

## Next Steps (PM action)

1. Re-dispatch to `@fullstack-dev` with fix scope above (F-001 + F-002 + W-1) on `iteration/v1.38` directly (no new feature branch needed — this is a same-iteration fix).
2. After fix commit lands, dispatch targeted re-review to `@qc-specialist` (`QC re-review: targeted — reviewers: qc-specialist`).
3. If QC1 re-review verdict is `Approve`, register residuals above in `status.json`, then mark Plan2 `Done`, then commit Done.
4. Proceed to Plan3 dispatch.

## Status Update (chat-only, NOT a file)

- Plan1: Done (commit `3f72b085`).
- Plan2: implementation complete (`ffeb0adc`), merged to integration (`2abbaa1a`), QC tri-review committed (`d33a65d4` / `65bf84d6` / `f4c92693`); consolidated verdict `Request Changes`; fix-now scope dispatched.
- Plan3: Todo, dispatch pending Plan2 closeout.
- Iteration: `iteration/v1.38` active; no PR to `main` yet.
