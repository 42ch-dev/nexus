---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-14-v1.46-research-auto-chain-e2e"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk (hermetic test harness design, supervisor tick test contracts, test module organization)
- Report Timestamp: 2026-06-15T02:45:00+08:00

## Scope
- plan_id: `2026-06-14-v1.46-research-auto-chain-e2e`
- Review range / Diff basis: `merge-base: 1d776d23 (P2 Done commit, base of P3 work) → tip: 87f00619 (P3 merge) (1 commit + 1 --no-ff merge = 2 total)` — equivalent `git diff 1d776d23..87f00619` or `git show --stat 1d776d23..87f00619`
- Working branch (verified): `iteration/v1.46`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (`git rev-parse --show-toplevel`); current HEAD `cae189c5`; P3 merge `87f00619` is an ancestor
- Files reviewed: 1 (`crates/nexus-orchestration/tests/research_supervisor_e2e.rs`, +530 lines, purely additive)
- Commit range (identical to Review range line): `1d776d23..87f00619`
- Tools run:
  - `git diff 1d776d23..87f00619 --stat`
  - `git show --stat 1d776d23..87f00619`
  - `cargo test -p nexus-orchestration --test research_supervisor_e2e` → 5 passed, 0 failed
  - `cargo test -p nexus-orchestration` → all green (regression check on host crate)
  - `cargo clippy -p nexus-orchestration --test research_supervisor_e2e -- -D warnings` → clean (P3 file in isolation)
  - `cargo +nightly fmt --all --check` → clean (exit 0)
  - Pre-existing clippy verification: `CARGO_TARGET_DIR=… cargo +nightly clippy -p nexus-orchestration --tests -- -D warnings` on `origin/main` HEAD `63b36a32` (detached worktree) → 60 errors, all in `tasks/mod.rs` (`doc_markdown`) + `worker/registry.rs` (`manual_assert_eq`)

## Independent Verification: Pre-existing Clippy Claim (PM-override `R-V145-PRE-CLIPPY-001`)

Per the `.mstar/AGENTS.md` § "Pre-existing claim verification protocol", I verified the claim against **current `origin/main` HEAD**:

- `git fetch origin main` → `origin/main` HEAD = `63b36a32` (V1.45 post-merge cleanup).
- Added detached worktree at `/tmp/v146-qc-clippy-verify-*` on `origin/main`.
- Ran `cargo +nightly clippy -p nexus-orchestration --tests -- -D warnings` (with isolated `CARGO_TARGET_DIR` to avoid the `/tmp` sandbox filesystem issue).
- Result: **`error: could not compile \`nexus-orchestration\` (lib test) due to 60 previous errors`** — failures concentrated in:
  - `crates/nexus-orchestration/src/tasks/mod.rs` — `clippy::doc_markdown` (e.g. lines 2093, 2742, 2746).
  - `crates/nexus-orchestration/src/worker/registry.rs` — `clippy::manual_assert_eq` (e.g. line 302).
- Cleanup: `git worktree remove … --force && git worktree prune && rm -rf <target cache>`.

**Conclusion**: Pre-existing claim is **TRUE**. The ~60–66 clippy errors in `nexus-orchestration` exist on V1.45 main (`63b36a32`) and are not introduced by V1.46 P3. PM-override `R-V145-PRE-CLIPPY-001` (decision `risk-accepted`) is valid. Per the assignment's hard rules, these pre-existing failures are **not** raised as V1.46 P3 findings. The new P3 test file (`research_supervisor_e2e.rs`) is clippy-clean in isolation, confirming P3 introduces zero new clippy issues.

## Findings

### 🔴 Critical

_None._

### 🟡 Warning

_None._

### 🟢 Suggestion

#### S-1 — `preset_version = 2` magic number duplicated across assertion and seed SQL
- **Triggering condition**: `research_preset_loads_and_structurally_valid` asserts `loaded.version == 2` (line 184), and `insert_research_schedule` hardcodes `preset_version = 2` in the INSERT statement (line 123). Both must change in lockstep when the embedded `research` preset's version bumps.
- **Impact (maintainability)**: A future preset-version bump could pass the load assertion (because the loader reads the new version) yet silently mismatch the seeded schedule row (because the SQL still writes `2`), or vice-versa. The risk is bounded because both tests would have to drift simultaneously, but the coupling is currently enforced only by the test author's discipline.
- **Fix (low priority; defer or accept)**: Derive the seed `preset_version` from `load_embedded_preset("research", &caps).unwrap().version` inside `insert_research_schedule` (or factor a `const RESEARCH_PRESET_VERSION: u32` derived from the loader at test setup), so the seed and the assertion reference a single source.
- **Confidence**: High.

#### S-2 — `format!("{g:?}").contains(...)` Debug-representationubstring match for preset gate inspection
- **Triggering condition**: In `research_preset_loads_and_structurally_valid` (lines 258–270), preset gates are checked via `gates.iter().any(|g| { let s = format!("{g:?}"); s.contains("intake_status") })` and `s.contains("work_ref")`.
- **Impact (maintainability)**: The check depends on the `Debug` impl of the gate enum, which is not a stable API contract. A seemingly-innocent refactor (e.g. renaming a variant or restructuring the enum) could break the test without changing semantics, or — worse — pass the test while no longer matching the intended gate. The check is also imprecise: a future gate that mentions `intake_status` in its debug output for an unrelated reason would silently satisfy the predicate.
- **Mitigating context**: The gate enum currently has no direct `field_name()`/`kind()` accessor, so structural pattern-matching would require an enum-match block; the existing `Debug`-substring approach is the pragmatic compromise and is consistent with how the test communicates intent (via the second `assert!` arg). The actual gate *semantics* are tested in `stage_gates.rs` (`check_stage_advance`).
- **Fix (low priority; defer or accept)**: If a `PresetGate::field_name()` (or `kind()`) accessor is added in a future contracts refactor, replace the substring match with a direct comparison. Until then, the current approach is acceptable.
- **Confidence**: Medium.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| S-1 | manual-reasoning | `crates/nexus-orchestration/tests/research_supervisor_e2e.rs:123` (SQL `preset_version = 2`) + `:184` (`assert_eq!(loaded.version, 2, …)`) | High |
| S-2 | manual-reasoning | `crates/nexus-orchestration/tests/research_supervisor_e2e.rs:258-270` (`format!("{g:?}").contains(...)`) | Medium |

## Architecture / Maintainability Assessment

### Hermetic boundary design — ✅ strong
- Module doc (`tests/research_supervisor_e2e.rs:1-24`) crisply states the boundary: **no network, no live ACP, no live LLM**; the ACP-dependent preset state machine is stubbed at the `on_schedule_terminal(Completed)` hook.
- Per-test doc comments restate the stub boundary at the point of invocation (e.g. headline E2E doc at `:377-392` walks through the live flow vs. the hermetic shortcut, identifying exactly which steps are skipped).
- In-line comment at the stub invocation (`:431-434`) repeats the boundary rationale locally — a future maintainer reading just that block understands why the preset state machine is bypassed.
- `test_pool()` uses a unique tempfile prefix `research_sup_e2e_` (line 45) — no collision risk with sibling tests; pattern-parity with `tests/auto_chain.rs` (`:67-74`).

### Test contract crispness — ✅ strong
- Test names are self-documenting and map 1:1 to R-V139P5-S1's sub-claims:
  - `research_preset_loads_and_structurally_valid` — T1 header contract.
  - `research_schedule_request_preset_input_contract` — T1 schedule-row seed contract.
  - `research_supervisor_tick_admits_pending_schedule` — T2 boot admission.
  - `research_supervisor_tick_drives_boot_to_terminal_done` — T2 headline E2E.
  - `research_schedule_boot_resume_running_to_paused_to_resumed` — T2 boot-resume recovery (second leg of R-V139P5-S1).
- Every `assert_eq!`/`assert!` carries a third-argument message documenting what is being asserted (e.g. `"research schedule must reach terminal status \`completed\` (= preset \`done\`)"`). The test reads as a contract document.
- The boot-resume test asserts idempotency of `resume_running_as_paused` (lines 509–514) — a deliberate correctness check, not just a happy-path walk-through.

### Pure-function factorization — ✅ appropriate
- `research_ready_work(&str, &str) -> WorkRecord` and `research_work_fields(&str, &str) -> WorkFields` are pure constructors with no I/O. Each test builds its own fixture; no shared mutable setup.
- DB helpers (`test_pool`, `seed_work`, `insert_research_schedule`, `schedule_status`) are isolated in their own `// ── Hermetic DB helpers ──` section, clearly demarcated from the test bodies.
- The `test_pool()` `std::mem::forget(db)` leak pattern mirrors `auto_chain.rs:74` — pre-existing test convention, not a regression. The doc comment (`:39-42`) calls this out explicitly.

### Module organization — ✅ appropriate
- 530 lines for 5 E2E tests is reasonable. The file is segmented with clear `// ── T1: … ──` / `// ── T2: … ──` banners that map to the plan's task structure.
- Splitting into T1/T2 modules would scatter the hermetic helpers across submodules for little gain. Current flat layout keeps the hermetic boundary visible in one place.

### Production-API surface exercised — ✅ correct
- Test exercises only public APIs: `load_embedded_preset`, `build_schedule_for_stage`, `ScheduleSupervisor::{new, tick, on_schedule_terminal, status_of, resume_running_as_paused, resume_schedule}`, `auto_chain::set_driver`. No test-only overrides in production code.
- The headline E2E wires the schedule as the Work's driver via `auto_chain::set_driver` (`:413-421`) — matching production daemon setup — so `on_schedule_terminal`'s `find_work_for_driver` path (supervisor.rs:414) actually runs end-to-end. Verified that `find_work_for_driver` is the function invoked; the test comment is accurate.

### Acceptance Criteria verification (plan §4)
| AC | Status | Evidence |
|----|--------|----------|
| 1. Integration test passes in CI without network/ACP | ✅ | `cargo test -p nexus-orchestration --test research_supervisor_e2e` → 5 passed; module doc asserts hermeticity |
| 2. Schedule row reaches expected terminal status | ✅ | `research_supervisor_tick_drives_boot_to_terminal_done` asserts `"completed"` row + `ScheduleStatus::Completed` + `terminated_at IS NOT NULL` |
| 3. Preset input assertions documented in test name/comments | ✅ | Module doc `:1-24`; per-test doc comments; `research_schedule_request_preset_input_contract` documents `references_dir`/`output_dir` boundary |
| 4. Residual R-V139P5-S1 closed in P-last | ⏳ deferred | Plan §4 AC4 explicitly defers close to P-last; not in P3 scope |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: **Approve**

P3 ships a tightly-scoped, well-documented hermetic supervisor E2E that closes R-V139P5-S1's lifecycle coverage (boot admission, terminal transition with `terminated_at` stamping, boot-resume recovery with idempotency). The hermetic boundary is documented at three levels (module doc, per-test doc, in-line comment at the stub site) and is faithful to Grill #10 (R-V139P5-S5 artifact E2E requiring ACP mock is OUT). The implementation is purely additive (1 new test file, no production code changes), introduces no new clippy issues (verified in isolation), and follows existing `auto_chain.rs` test conventions for consistency. Pre-existing clippy failures in `nexus-orchestration` (`tasks/mod.rs` + `worker/registry.rs`) are independently verified against `origin/main` HEAD `63b36a32` and covered by PM-override `R-V145-PRE-CLIPPY-001` — not raised here.

The two Suggestions (S-1: `preset_version` magic-number coupling; S-2: `Debug`-substring gate inspection) are low-impact maintainability refinements that do not block approval and may be deferred or accepted at PM's discretion.
