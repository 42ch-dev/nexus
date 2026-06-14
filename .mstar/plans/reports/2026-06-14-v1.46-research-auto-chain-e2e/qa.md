# QA Report — V1.46 P3 research-auto-chain-e2e

## Scope tested

- **plan_id**: `2026-06-14-v1.46-research-auto-chain-e2e`
- **QA mode**: Default (full verification)
- **Working branch (verified)**: `iteration/v1.46`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Review range / Diff basis**: `merge-base: 1d776d23 (P2 Done commit, base of P3 work) → tip: 9cbb1002 (P3 + fix merge)` (qc3 revalidation HEAD `1978f08b` is docs-only on top). Equivalent to `git diff 1d776d23..9cbb1002`.
- **Implementation artifacts**: 1 new file `crates/nexus-orchestration/tests/research_supervisor_e2e.rs` (546 lines after W-1 fix; 5 hermetic tests).
- **QC cycle**: qc1 (Approve at `4135a467`), qc2 (Approve at `412f37cb`), qc3 initial Request Changes (W-1), targeted re-review Approve at `1978f08b` after W-1 fix commit `a3fb417a` + merge `9cbb1002`.
- **All three QC seats**: Approve.

## Acceptance criteria evidence

### Original P3 acceptance criteria (plan §4)

1. **Integration test passes in CI without network/ACP** — ✅
   - `cargo test -p nexus-orchestration --test research_supervisor_e2e` → 5 passed, 0 failed (0.13s).
   - Module doc (lines 1-25) and per-test docs explicitly state hermetic boundary: "no network, no live ACP, no live LLM".
   - Imports in `research_supervisor_e2e.rs`: only `nexus_contracts`, `nexus_local_db`, `nexus_orchestration`, `sqlx::SqlitePool`. No `reqwest`, `hyper`, `tokio::net`, `tcp`, `udp`, or any network crate.
   - `rg` scan for network patterns against the file returns empty (confirmed via source inspection + test execution).

2. **Schedule row reaches expected terminal status** — ✅
   - Headline test `research_supervisor_tick_drives_boot_to_terminal_done` (lines 410-479):
     - Boot: `sup.tick()` → schedule status `"running"`.
     - Terminal stub: `sup.on_schedule_terminal(..., ScheduleStatus::Completed)`.
     - Asserts DB row: `"completed"` (matches preset `terminal: "done"`).
     - `sup.status_of(...)` returns `ScheduleStatus::Completed`.
     - `terminated_at IS NOT NULL` stamped (full UPDATE path, not flag flip).
   - Evidence from run: all 5 tests pass, including this one.

3. **Preset input assertions documented in test name/comments** — ✅
   - Test names are self-documenting:
     - `research_preset_loads_and_structurally_valid` (T1 header + gate contract).
     - `research_schedule_request_preset_input_contract` (T1 schedule-row seed contract).
   - Comments explicitly document the input contract:
     - Lines 295-300: "NOTE: the research prompt templates additionally consume `{{preset.input.references_dir}}` ... Those paths are resolved from the Work's workspace at session-run time and are NOT part of the schedule-row seed".
     - Lines 335-342: asserts `references_dir` / `output_dir` are absent from seed (runtime-resolved boundary).
     - Gate assertions (lines 256-286 post-fix) document: "research gate: intake_status == \"complete\"" and "research gate: work_ref required".
   - `research_work_fields` + `build_schedule_for_stage` assertions cover `work_id`, `fl_e_stage`, `creative_brief`, `inspiration_log`, `work_ref`.

4. **Residual R-V139P5-S1 closed in P-last (lifecycle deferred)** — ✅ addressed in code
   - T1+T2 implementation delivers exactly the 5 hermetic tests that close the supervisor+boot E2E coverage gap (boot admission, terminal transition with `terminated_at`, boot-resume recovery with idempotency).
   - Plan §4 AC4 and qc-consolidated explicitly defer **lifecycle** closure in `status.json` to P-last. P3 ships the test artifact; residual remains open for P-last bookkeeping (not a P3 failure).

### Fix-round acceptance criteria (W-1 from qc-consolidated + qc3 reval)

- **W-1**: Debug-substring assertion replaced with **typed pattern matching** on `nexus_contracts::local::orchestration::preset_gate::{Gate, GateOp}` — ✅
  - Pre-fix (qc3 finding): `gates.iter().any(|g| { let s = format!("{g:?}"); s.contains("intake_status") })` etc. (lines 258-270 original).
  - Post-fix (commit `a3fb417a`, merge `9cbb1002`): import added; full match block (lines 265-281):
    ```rust
    for g in gates {
        match g {
            Gate::WorkField {
                field,
                op: GateOp::Equals { value },
            } if field.as_str() == "intake_status" && value.as_str() == Some("complete") => {
                has_intake_status_complete = true;
            }
            Gate::WorkField {
                field,
                op: GateOp::Required,
            } if field.as_str() == "work_ref" => {
                has_work_ref_required = true;
            }
            _ => {}
        }
    }
    assert!(has_intake_status_complete, "research gate: intake_status == \"complete\"");
    assert!(has_work_ref_required, "research gate: work_ref required");
    ```
  - Both semantic assertions preserved exactly.
  - This also resolves qc1 S-2 (same Debug-substring brittleness).
  - Diff slice `87f00619..9cbb1002 -- crates/nexus-orchestration/` touches **only** `research_supervisor_e2e.rs` (1 file, gate assertion block only).

## Spec / scope discipline

- Checkout alignment (verified at start of session):
  - `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`
  - `git branch --show-current` → `iteration/v1.46`
  - `git log -1 --oneline` → `1978f08b qc(v1.46-p3): qc3 revalidation (targeted re-review)`
  - `git status --short` → (empty; working tree clean)
- `git diff 1d776d23..9cbb1002 --stat` confirms full P3+fix+QC cycle scope (10 files including new test + P4 work + docs/status). P3-only code slice scoped to `git diff 87f00619..9cbb1002 -- crates/nexus-orchestration/` (only the test file for W-1).
- `status.json` (at HEAD): P3 plan row present with `status: "InReview"`. `residual_findings["2026-06-14-v1.46-research-auto-chain-e2e"]` contains exactly the 3 open P3 residuals (R-V146P3-QC1-S1, R-V146P3-QC3-S1, R-V146P3-QC3-S2). Pre-existing bucket `R-V145-PRE-CLIPPY-001` (1). Assignment context: 18 P0/P1/P2 residuals remain tracked separately for the iteration. Total for this QA scope per assignment: 3 + 1 + 18 = 22. No P3 residuals closed by this round (correct; deferred per qc-consolidated).
- No production code changes in P3 (purely additive test file + 1-line surgical W-1 fix). No schema/CLI surface touched.
- Pre-existing clippy (`R-V145-PRE-CLIPPY-001`) independently verified by QC1/QC2/QC3 against `origin/main` `63b36a32`; **not raised** as P3 finding (PM-override respected; `cargo clippy -p nexus-orchestration --test research_supervisor_e2e -- -D warnings` clean in isolation).
- No network/ACP/LLM in test scope (per plan non-goals and Grill #10).

## Findings

- **None blocking**. All mandatory CI gates pass. All original P3 ACs and the W-1 fix AC are satisfied with reproducible evidence.
- 3 low-severity P3 residuals (R-V146P3-QC1-S1 magic-number coupling; R-V146P3-QC3-S1 raw SQL fixture; R-V146P3-QC3-S2 `fetch_one` panic) remain open per qc-consolidated disposition — correctly not addressed in this round.
- 1 pre-existing clippy residual (R-V145-PRE-CLIPPY-001) remains open (PM-override, V1.46 P-last hygiene responsibility).
- 18 P0/P1/P2 residuals tracked separately (out of P3 scope).

## Recommended owners

- P3 residuals (S-1/S-2 style maintainability): defer to V1.46+ per qc-consolidated; owner TBD at P-last or next hygiene plan.
- Pre-existing clippy (R-V145-PRE-CLIPPY-001): V1.46 P-last hygiene plan (per assignment hard rules).
- R-V139P5-S1 lifecycle close: P-last (per plan §4 AC4).

## Reproduction steps

1. Checkout: `git checkout iteration/v1.46` (at or after `1978f08b`).
2. Verify alignment: `git rev-parse --show-toplevel`, `git branch --show-current`, `git log -1 --oneline`, `git status --short`.
3. P3 E2E: `cargo test -p nexus-orchestration --test research_supervisor_e2e` (expect 5 passed).
4. Full suite: `cargo test --all` (expect all green).
5. Isolation clippy: `cargo clippy -p nexus-orchestration --test research_supervisor_e2e -- -D warnings` (expect clean).
6. Fmt: `cargo +nightly fmt --all --check` (expect exit 0, silent).
7. W-1 fix verification: `git show 9cbb1002 -- crates/nexus-orchestration/tests/research_supervisor_e2e.rs | head -100` (or read lines 256-286); confirm typed `match` on `Gate`/`GateOp`, no `format!("{g:?}").contains`.
8. AC2 terminal evidence: run the specific test and grep output for `completed` + `terminated_at`.
9. Residual counts: `python3 -c '...' < .mstar/status.json` (3 P3 + 1 pre + context 18).
10. Diff discipline: `git diff 1d776d23..9cbb1002 --stat` and scoped `git diff 87f00619..9cbb1002 -- crates/nexus-orchestration/`.

All commands above were executed in this session on the verified checkout; outputs match the evidence sections.

## Not tested

- R-V139P5-S5 (artifact E2E requiring ACP mock) — explicitly out of scope per plan non-goals and Grill #10.
- Live preset state-machine execution (ACP/LLM-dependent) — stubbed at `on_schedule_terminal` boundary.
- P4 work (`pool-observability`) or any other concurrent plan.
- Multi-volume plans, new preset syntax, orchestration engine changes.
- Production daemon end-to-end with real ACP (hermetic DB only).
- Clippy `--all` (pre-existing on V1.45 main; PM-override; P3 file clean in isolation).

## QA Verdict

**PASS**

All acceptance criteria (original P3 + W-1 fix) are satisfied with reproducible command output, source inspection, and git evidence. Checkout, scope discipline, CI gates, and residual tracking align with the Assignment and qc-consolidated disposition. The hermetic supervisor E2E for the research preset is verified end-to-end. P3 is ready for PM to mark `Done` (leaving the 3 P3 residuals + pre-existing + P0/P1/P2 as documented).
