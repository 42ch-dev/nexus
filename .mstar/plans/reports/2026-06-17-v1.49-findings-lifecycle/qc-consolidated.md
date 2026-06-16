---
report_kind: qc-consolidated
plan_id: 2026-06-17-v1.49-findings-lifecycle
generated_at: 2026-06-17T18:45:00+08:00
review_range: 1fd3a9c4..04608722
working_branch: iteration/v1.49
qc_reports:
  - .mstar/plans/reports/2026-06-17-v1.49-findings-lifecycle/qc1.md (qc-specialist, Request Changes)
  - .mstar/plans/reports/2026-06-17-v1.49-findings-lifecycle/qc2.md (qc-specialist-2, Request Changes)
  - .mstar/plans/reports/2026-06-17-v1.49-findings-lifecycle/qc3.md (qc-specialist-3, Approve)
verdict: Request Changes
---

# V1.49 P0 — Findings Lifecycle QC Consolidated Report

## Verdict: Request Changes

Two of three reviewers raised the same blocker (W-1) on the PATCH handler error classification. The third reviewer (QC3, performance/reliability) approved with 0/0/7.

## Findings Roll-up

| Severity | qc1 | qc2 | qc3 | Total | Consolidated |
|----------|-----|-----|-----|-------|--------------|
| 🔴 Critical | 0 | 0 | 0 | 0 | — |
| 🟡 Warning | 1 | 1 | 0 | 1 unique | **W-1 (blocking)** |
| 🟢 Suggestion | 3 | 4 | 7 | 11 unique | tracked; see below |

### W-1 — Uniform `ConstraintViolation → INVALID_TRANSITION` remap collapses distinct failure classes (raised by qc1 + qc2; qc3 noted same as S-1)

- **Locations**:
  - `crates/nexus-daemon-runtime/src/api/handlers/findings.rs::update_finding_handler` lines 319–335
  - DAO emission sites: `crates/nexus-local-db/src/findings.rs` lines 670 (transition), 714–743 (enum membership)
  - 422 mapping: `crates/nexus-daemon-runtime/src/api/errors.rs` line 172
- **Root cause**: `LocalDbError::ConstraintViolation` is a single variant emitted for at least four distinct conditions (illegal transition, invalid severity, invalid `target_executor`, unknown status membership). The PATCH handler maps every occurrence to `BadRequest { code: "INVALID_TRANSITION", message: <raw DAO constraint text> }`. The test `findings_lifecycle_rejects_unknown_status_value` explicitly documents the uniform remap.
- **Impact**:
  - **Maintainability** (qc1 W-1): the stable public code `INVALID_TRANSITION` is semantically incorrect for non-transition failures; no guardrail against drift as new validated fields are added.
  - **Security / client reasoning** (qc2 W-1): clients that key off `error.code == "INVALID_TRANSITION"` to detect lifecycle policy violations will also fire for simple enum typos. Raw DAO constraint text (table name "findings", exact wording) leaks on the public error surface. The same 422 code is used for enum-membership failures and transition-legality failures, masking distinct probing classes.
  - **Performance** (qc3 S-1, non-blocking): the false-positive risk is real for clients but currently no client pattern-matches on the granular distinction; flagged as follow-up.
- **Fix** (PM decision: **DAO-level split — preferred** per qc1 + qc2):
  1. Split `LocalDbError::ConstraintViolation` into two variants:
     - `IllegalTransition { from: String, to: String }` (emitted by `enforce_status_transition`)
     - `InvalidEnum { field: &'static str, value: String, allowed: &'static [&'static str] }` (emitted by the three enum-membership sites in `update_finding`)
  2. In `update_finding_handler`:
     - `IllegalTransition { from, to }` → `BadRequest { code: "INVALID_TRANSITION", message: format!("invalid status transition '{}' → '{}'", from, to) }`
     - `InvalidEnum { field, value, allowed }` → `BadRequest { code: "INVALID_INPUT", message: format!("invalid {} value '{}'; allowed: {}", field, value, allowed.join(", ")) }`
  3. Add error mapping in `errors.rs::status_code()` and `error_code()` (both variants → 422).
  4. Add `tracing::warn!` (qc3 S-2) in both arms with structured fields (creator_id, finding_id, the structured error).
  5. Add hermetic tests:
     - DAO: `illegal_transition_emits_typed_error`, `invalid_enum_emits_typed_error`
     - Handler: `findings_lifecycle_distinguishes_invalid_transition_from_invalid_enum` (verifies 422 + distinct codes for: bad severity, bad target_executor, unknown status word, illegal transition).
- **Acceptance criteria**:
  1. Public error taxonomy distinguishes transition-legality from enum-membership failures (two distinct `error.code` values).
  2. No DAO constraint-string prefix matching in handler (no string-sniffing for "invalid status transition" / "invalid severity" / etc.).
  3. `message` field no longer includes internal table name "findings"; structured for human + programmatic use.
  4. Existing 11 handler tests still pass; 2+ new tests cover the distinction.
  5. CI gates clean: `cargo +nightly fmt --all --check` + `cargo clippy --all -- -D warnings`.

### Other Suggestions (non-blocking; tracked)

Suggestions from qc1, qc2, qc3 are all documentation / observability / future-hardening items:

| # | Source | Title | PM action |
|---|--------|-------|-----------|
| S-1 qc1 | qc1 | Self-loop rejection `from == to` — document in API surface | Document in handler docstring (sibling of W-1 fix) |
| S-2 qc1 | qc1 | Document actionable-set scope boundary on `count_open_findings_by_severity` and `list_stale_open_findings` | One-line comment per function |
| S-3 qc1 | qc1 | `enforce_status_transition` TOCTOU — note CAS alternative | Inline docstring enhancement |
| S-2 qc3 | qc3 | No `tracing::warn!` on transition rejection (observability gap) | Roll into W-1 fix (step 4) |
| S-3 qc3 | qc3 | TOCTOU in `update_finding` | Same as S-3 qc1 (perf lens) |
| S-4 qc3 | qc3 | `is_valid_status` not `const fn` | Documented in code; no action |
| S-5 qc3 | qc3 | `ANALYZE findings` cost | Verified acceptable; no action |
| S-6 qc3 | qc3 | CLI `?status=open` gap (R-V149P0-01) | Already tracked; defer to V1.50 |
| S-7 qc3 | qc3 | SQLx cache rename determinism | Verification, no action |
| S-1 qc2 | qc2 | TOCTOU window — single-statement CAS | Same as qc1 S-3 / qc3 S-3 |
| S-2 qc2 | qc2 | Negative-path / adversarial coverage for PATCH surface | Add 1 malformed-status injection test alongside W-1 fix |
| S-3 qc2 | qc2 | `duplicate` / `in_review` semantics — document "hiding" lever | One-line note in handler docstring |
| S-4 qc2 | qc2 | R-V149P0-01 cross-reference | Already tracked |

**PM decision**: fold S-1 qc1, S-2 qc1, S-3 qc1, S-2 qc3, S-2 qc2, S-3 qc2 into the W-1 fix wave (low-cost, same files, same commit). Leave the rest for follow-up (no action needed for S-4, S-5, S-7 qc3, S-1 qc2/3, S-6 qc3 which is already a tracked residual).

## Residual registration

- **R-V149P0-02 (medium)** — `LocalDbError::ConstraintViolation` overloading leaks DAO internals + collapses distinct failure classes on the PATCH public surface.
  - **Where**: `crates/nexus-local-db/src/findings.rs::update_finding` and the `ConstraintViolation` emission sites; `crates/nexus-daemon-runtime/src/api/handlers/findings.rs::update_finding_handler` (lines 319–335).
  - **Decision**: **fix in current wave** (this is the consolidated W-1).
  - **Owner**: `@fullstack-dev` (fix wave).
  - **Target**: V1.49 P0.
  - **Source**: qc1 W-1 + qc2 W-1 + qc3 S-1 (cross-references).
  - **Closure**: blocked on W-1 fix merge.

## Pre-existing residuals (unrelated to P0; not in this wave)

- `R-V149P0-01` (medium) — CLI `assemble_open_findings_block` still uses `?status=open`. **Defer to V1.50** (out of P0 scope per compass §0.1 #8 — would require new wire contract).

## Next step

PM dispatches **targeted fix wave** to `@fullstack-dev` on a new fix branch from `iteration/v1.49` @ `8a809ab3` (current integration HEAD with all 3 QC reports). The fix must:

1. Apply the DAO-level split (preferred fix per QC consensus).
2. Add `tracing::warn!` on both arms (q3 S-2).
3. Update the test that documented the uniform remap (`findings_lifecycle_rejects_unknown_status_value`) to assert the new `INVALID_INPUT` code.
4. Add 1 malformed-status injection test (q2 S-2) — minimum, can be combined with the new distinction tests.
5. Fold in the 3 small docstring additions (q1 S-1, S-2, S-3 + q2 S-3).
6. Re-run CI gates.

After fix wave:
- PM merges fix branch to `iteration/v1.49`.
- QC1 + QC2 do **targeted re-review** (N=2; qc3 stays approved per `mstar-review-qc` default — they raised no blocking finding). Each updates the **same** `qc1.md` / `qc2.md` (no `qc1-rev2.md`).
- If re-review approves: PM dispatches `@qa-engineer` for the QA pass on the same `Review cwd` + `plan_id` + `Review range` / `Diff basis` (extended to cover the fix commits).
- After QA passes: PM marks plan `Done`, transitions to P1.

PM notes for tracking:

- `feature/v1.49-findings-lifecycle` worktree (`.worktrees/v1.49-findings-lifecycle`) is left intact for QC review context. The fix wave will use a NEW worktree (e.g. `.worktrees/v1.49-p0-w1-fix`) on a new fix branch.
- `R-V149P0-02` is registered in root `residual_findings[2026-06-17-v1.49-findings-lifecycle]` with `decision: fix-in-wave` (per PM; closure will move to `archived/residuals/<plan-id>.json` on fix merge).
