---
report_kind: qa
reviewer: qa-engineer
plan_id: "2026-06-07-v1.37-novel-foundation-first"
verdict: "Approve"
generated_at: "2026-06-07T18:45:58Z"
working_branch: "feature/v1.37-novel-foundation-first"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus"
review_range: "full branch merge-base(iteration/v1.37)..HEAD (HEAD = cff692f5)"
---

# QA Verification Report — V1.37 P0 Novel Foundation-First UX Hardening

## Scope

- **plan_id**: 2026-06-07-v1.37-novel-foundation-first
- **Review range / Diff basis**: full branch `merge-base(iteration/v1.37)..HEAD` (assignment HEAD = `cff692f5`)
- **Working branch verified**: `feature/v1.37-novel-foundation-first`
- **Review cwd verified**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Latest commit at start**: `cff692f50b39acad01be83b5454ee47f580410a0 qc(v1.37-p0): targeted re-review of F-002`
- **QA mode**: PM-scheduled final QA verification; report-only plus report commit; no product-code changes.

## Acceptance Criteria Verification

| AC | Result | Evidence |
| --- | --- | --- |
| AC-1: `novel-writing` without scaffold returns `preset_gates_failed` 422 | Pass | `cargo test -p nexus-daemon-runtime --test fl_e_schedule_api gate_failure_returns_422_with_structured_body` passed (`1 passed; 0 failed`). Code review of `schedules.rs` confirms gated presets with missing `body.input.work_id` return `StatusCode::UNPROCESSABLE_ENTITY` with serialized `PresetGatesFailed { error: "preset_gates_failed", ... }`; evaluated gate failures and missing work rows also return 422 structured bodies. |
| AC-2: `novel-project-init` input reaches `preset.input.*` and scaffold | Pass | `cargo test -p nexus-orchestration --test novel_project_init` passed (`19 passed; 0 failed`). The assignment-specified `cargo test -p nexus-orchestration novel_project_init` completed successfully but matched zero test names, so the integration-test binary was run explicitly as supplemental evidence. Code review confirms `novel-project-init/preset.yaml` maps `{{preset.input.creator_id}}`, `work_id`, `work_ref`, `title`, `total_planned_chapters`, `world_id`, and `fields_changed` into `novel.project_scaffold`; `novel_scaffold.rs` consumes these fields and renders `work_ref`, `title`, `world_id`, and `total_planned_chapters` into scaffold templates/DB writes. `schedules.rs::seed_core_context` persists `preset.input=<json>` into core context when request input is present. |
| AC-3: Failed scaffold cannot leave `work_chapters` rows without `works` PATCH | Pass | Code review confirms `novel_scaffold.rs` wraps `work_chapters::seed_chapters_tx(...)` and `works::patch_work_tx(...)` in one `pool.begin()` transaction and commits only after both succeed; errors before commit drop the transaction. Test coverage includes `t7g_db_failure_rolls_back_filesystem_scaffold` for mid-flight DB failure/FS rollback and `t7c`/`t7d` success-path DB seed+patch assertions. No separate T3-success/T4-fail simulation was found; single-transaction code review satisfies the AC atomicity condition. |
| AC-4: `--force-gates` explicit and audit-logged | Pass | `cargo test -p nexus-daemon-runtime --test fl_e_schedule_api` passed (`10 passed; 0 failed`), including `force_gates_writes_audit_row`, `force_gates_without_reason_is_rejected`, `force_gates_with_long_reason_rejected`, and `force_gates_with_ansi_in_reason_rejected`. Code review confirms reason presence, max length, ANSI/control rejection, transactional audit insert via `nexus_local_db::insert_force_gates_audit`, and schedule insert in the same transaction. Supplemental `sqlite3 .sqlx/state.db` confirmed the `force_gates_audit` table exists; the offline `.sqlx/state.db` query returned count `0`, as expected because hermetic tests assert against their own DB helpers. |
| AC-5: Tests cover success + failure paths | Pass | Relevant test commands passed: `cargo test -p nexus-orchestration gate` (`58 passed; 0 failed` across matched gate-related tests), `cargo test -p nexus-orchestration --test novel_project_init` (`19 passed; 0 failed`), `cargo test -p nexus-daemon-runtime --test fl_e_schedule_api` (`10 passed; 0 failed`), `cargo test -p nexus42` (`746 passed; 0 failed`), and `cargo test -p nexus-local-db` (`148 passed; 0 failed`). Coverage includes gate failures, scaffold success/failure/idempotency, CLI command surface flags, audit table insert/list/prune, and migrations. |
| AC-6: Residual closure/registration state | Pass | `python3 -m json.tool .mstar/status.json > /dev/null` passed. `status.json` has `metadata.tech_debt_summary.v1.36_added.closed_in_v1.37` entries for `R-V136P1-01`, `R-V136P1-02`, and `R-V136P3-02`, and plan metadata `closed_residuals` repeats those closures. Root `residual_findings["2026-06-07-v1.37-novel-foundation-first"]` contains new `R-V137P0-01`. |

## CI / Gate Evidence

| Command | Result |
| --- | --- |
| `cargo +nightly fmt --all -- --check` | Pass; no formatter diff emitted. |
| `cargo clippy -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` | Pass; `Finished dev profile` with exit 0. |
| `cargo test -p nexus-orchestration gate` | Pass; 58 matched tests passed, 0 failed. One non-deny warning from an unrelated test target appeared (`unused variable: ctx` in `e2e_novel_writing.rs`) but did not fail this non-clippy test command. |
| `cargo test -p nexus-orchestration --test novel_project_init` | Pass; 19 passed, 0 failed. |
| `cargo test -p nexus-daemon-runtime --test fl_e_schedule_api` | Pass; 10 passed, 0 failed. |
| `cargo test -p nexus42` | Pass; 746 passed, 0 failed (including doc-tests: 1 passed, 1 ignored). |
| `cargo test -p nexus-local-db` | Pass; 148 passed, 0 failed (including doc-tests: 2 passed). |

**Total relevant tests counted for this QA pass**: 981 passed / 0 failed.

## QC Tri-Review Confirmation

All three QC reports are present under `.mstar/plans/reports/2026-06-07-v1.37-novel-foundation-first/` and have final verdict `Approve`:

- `qc1.md`: YAML frontmatter `verdict: "Approve"`; final `Revalidation #2` updated verdict `Approve`.
- `qc2.md`: YAML frontmatter `verdict: "Approve"`; revalidation updated verdict `Approve`.
- `qc3.md`: YAML frontmatter `verdict: "Approve"`; revalidation updated verdict `Approve`.

## Notes / Non-blocking Observations

- The assignment's `cargo test -p nexus-orchestration novel_project_init` command is a test-name filter and did not execute the `tests/novel_project_init.rs` integration tests directly. I ran the exact command and then ran `cargo test -p nexus-orchestration --test novel_project_init` to verify the intended scaffold suite.
- No product code was modified by QA. This report is the only intended tracked change.

## Verdict

**Verdict**: Approve

All six acceptance criteria passed, all required relevant tests/gates passed, and all three QC reports have final `Approve` verdicts.
