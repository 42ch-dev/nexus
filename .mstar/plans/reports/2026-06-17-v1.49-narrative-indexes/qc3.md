---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-17-v1.49-narrative-indexes
verdict: Approve
generated_at: 2026-06-17
review_range: 3630a4e5..f448b658
working_branch: iteration/v1.49
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: MiniMax/MiniMax-M3
- Review Perspective: Performance and reliability (QC3)
- Report Timestamp: 2026-06-17

## Scope
- plan_id: `2026-06-17-v1.49-narrative-indexes`
- Review range / Diff basis: `3630a4e5..f448b658` (equivalent to `git diff 3630a4e5...f448b658`)
- Working branch (verified): `iteration/v1.49` @ `946cfba6` (head SHA drifted from the assignment-stated `d78d240b` because `qc-specialist-2` committed their report `946cfba6` mid-review; implementation scope `3630a4e5..f448b658` is unchanged)
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (from `git rev-parse --show-toplevel`)
- Files reviewed: 13 (4 P1 feature commits + 1 merge per `git log --oneline 3630a4e5..f448b658`):
  - `crates/nexus-orchestration/src/narrative_index.rs` (NEW, 919 lines, ~600 impl + ~317 tests)
  - `crates/nexus-orchestration/src/auto_chain.rs` (+178)
  - `crates/nexus-orchestration/src/stage_gates.rs` (+95)
  - `crates/nexus-orchestration/src/schedule/supervisor.rs` (+32)
  - `crates/nexus-orchestration/src/sync_module.rs` (+46 test)
  - `crates/nexus-orchestration/src/preset_ids.rs` (+20)
  - `crates/nexus-orchestration/src/lib.rs` (+1)
  - `crates/nexus-orchestration/embedded-presets/novel-writing/preset.yaml` (+2)
  - `crates/nexus-orchestration/embedded-presets/novel-writing/prompts/outline-chapter.md` (+7)
  - `crates/nexus-orchestration/embedded-presets/novel-writing/prompts/draft-chapter.md` (+9)
  - `crates/nexus-orchestration/tests/e2e_novel_writing.rs` (+5)
  - `.mstar/plans/reports/2026-06-17-v1.49-narrative-indexes/completion.md` (NEW, 115)
  - `.mstar/status.json` (residual additions R-V149P1-01/02)
- Commit range: `9e73a047`, `2ef52406`, `01425b8c`, `1f037243`, `f448b658` (merge)
- Tools run:
  - `cargo +nightly fmt --all --check` — **clean** (no diff output)
  - `cargo clippy -p nexus-orchestration -- -D warnings` — **clean** (Finished dev profile in 14.10s)
  - `cargo clippy --all -- -D warnings` — **clean** (CI gate, Finished dev profile in 18.01s)
  - `cargo test -p nexus-orchestration --lib narrative_index` — 25/25 pass (0.04s)
  - `cargo test -p nexus-orchestration --test novel_project_init` — 22/22 pass (2.22s)
  - `cargo test -p nexus-orchestration --test e2e_novel_writing` — 11/11 pass (0.02s)
  - `cargo test -p nexus-orchestration --test sync_module_works_layout` — 9/9 pass (0.01s)
  - `cargo test -p nexus-orchestration --test review_report` — flaky (~1-in-3 in same-binary full run; **pre-existing**, see S-5)
  - Flake reproduction on `origin/main @ be27111b` (per `.mstar/AGENTS.md` "Pre-existing claim verification protocol"): 2/10 failure rate on `cargo test -p nexus-orchestration --test review_report` — confirms R-V149P1-02 is pre-existing on `integration_merge_target` (`main`).

## Findings

### 🔴 Critical
*(none)*

### 🟡 Warning
*(none)*

### 🟢 Suggestion

#### S-1 — Promotion hook fan-out: O(N²) file reads over Work lifetime (V1.50 follow-up)
- **Where**: `crates/nexus-orchestration/src/auto_chain.rs::promote_outlines_in` (lines 1186–1242); called from `promote_foreshadowing_for_schedule` (line 1181) → `on_schedule_terminal` (line 459) on every `novel-writing` schedule terminal.
- **Detail**: Each `novel-writing` schedule covers one chapter's full FL-E flow. The terminal hook reads **all** `Outlines/chapters/*-outline.md` files in the Work and re-runs `promote_outline_to_index` per outline. For a Work with N chapters, after chapter k the hook reads k outlines. Cumulative file reads over the Work's lifetime: **1 + 2 + … + N = N(N+1)/2 = O(N²)**.
  - N=20 (typical V1.49 Work): 210 outline reads. ~21 ms.
  - N=50: 1275 reads. ~130 ms.
  - N=200: 20 100 reads. ~2 s.
  - N=1000 (very long Work): 500 500 reads. ~50 s.
  Each read is a 1–5 KiB markdown file plus a parse pass; cost is dominated by I/O syscalls, not parsing. Promotion is idempotent, so the per-chapter cost is bounded by newly-allocated ids, not by cumulative count.
- **Why Suggestion (not Warning)**: The implementer explicitly notes "an incremental (track promoted outlines) optimization is a V1.50 follow-up, not needed for MVP" (completion report §Risks/follow-ups). For V1.49's typical local-first Work sizes (≤ 50 chapters), the cost is sub-second and unobservable next to LLM API round-trips. The trade-off is sound.
- **Suggested follow-up (V1.50)**: track per-Work `last_promoted_outline_mtime` in the DB (or in a small `.foreshadowing_promoted` sibling file); only re-promote outlines newer than the index's mtime. The single-writer daemon assumption plus the existing atomic-write safety means a Work-local cursor is sufficient.

#### S-2 — `read_foreshadowing_summary` reads the file on every `build_preset_input` call (no caching)
- **Where**: `crates/nexus-orchestration/src/stage_gates.rs::build_preset_input` (lines 191–211); `crates/nexus-orchestration/src/narrative_index.rs::read_foreshadowing_summary` (lines 579–597).
- **Detail**: `build_preset_input` is called once per stage advance (CLI `creator run stage advance` and the daemon's per-state machine tick). The new `foreshadowing_summary` block invokes `read_foreshadowing_summary(&work_dir)` when `workspace_dir` and `work_ref` are both set. The function reads `foreshadowing.md` from disk, parses it, and formats a per-row summary on every call. There is no caching layer (no per-Work in-memory cache, no per-stage memoization).
  - For a Work with N chapters × ~3 stages/chapter ≈ 3N stage advances, this is **3N** reads of the index file.
  - Per-read cost: read ~5–10 KiB + parse ~100 rows + format `String` of ~5 KiB ≈ 100 µs.
  - 3 × 50 = 150 reads ≈ 15 ms (negligible).
  - 3 × 200 = 600 reads ≈ 60 ms (still negligible vs. LLM API round-trips).
- **Why Suggestion (not Warning)**: Cost is bounded and dominated by I/O, not by parse complexity. The index file is small (single-author single-Work, < 1 KiB typical, < 10 KiB worst case). No caching is required for V1.49 MVP; it would only matter for a V1.50 work where hundreds of stages hit `build_preset_input` in a tight loop.
- **Suggested follow-up (V1.50, optional)**: if a future code path makes `build_preset_input` significantly hotter (e.g. sub-stage context refresh), wrap `read_foreshadowing_summary` with a per-Work `RwLock<Option<(FileMeta, String)>>` cache keyed on the supervisor's `Arc<ScheduleSupervisor>` lifetime. Not needed today.

#### S-3 — `atomic_write` temp-file leak on `fs::rename` failure
- **Where**: `crates/nexus-orchestration/src/narrative_index.rs::atomic_write` (lines 557–564).
- **Detail**: `atomic_write` writes to `<index_path>.md.tmp` (via `set_extension("md.tmp")`), then `std::fs::rename` to the final path. The `?` after `fs::write` and after `fs::rename` propagates errors but **does not clean up the temp file**. Failure modes that leave the temp file on disk:
  - `fs::write` partially writes the file (process killed mid-write) → partial `<index>.md.tmp` left on disk; subsequent successful `fs::write` will overwrite it (self-healing).
  - `fs::rename` fails (cross-device link, target filesystem error, target is read-only, etc.) → complete temp file left at `<index>.md.tmp`; the next call also writes to the same deterministic name, so it's also self-healing on the **next** successful rename.
- **Why Suggestion (not Warning)**: The deterministic temp name (`foreshadowing.md.tmp`) means a successful subsequent call always overwrites the orphaned temp. No accumulation; no risk of stale data being renamed in. The leak window is bounded by the next promotion of the same Work, which is typically seconds-to-minutes later. The current design is correct for the MVP single-writer assumption.
- **Suggested follow-up (V1.50)**: use `tempfile::NamedTempFile::new_in(parent_dir)?.persist(target_path)?` (the `tempfile` crate is already a project dep — see `Cargo.toml:48`, `schedule/derivation.rs:772`, `tests/system_preset_e2e.rs:49`); `NamedTempFile`'s `Drop` impl cleans up the temp on early return. Bonus: enables concurrent-safe O_EXCL creation. Pairs naturally with the qc2-W-2 advisory-lock work (R-V149P1-01) when multi-writer support lands.

#### S-4 — No integration test for `promote_foreshadowing_for_schedule` (test coverage gap on the hot path)
- **Where**: `crates/nexus-orchestration/src/auto_chain.rs::promote_foreshadowing_for_schedule` (lines 1095–1183) and `promote_outlines_in` (lines 1186–1242). Integration tests under `crates/nexus-orchestration/tests/` cover `narrative_index` lib (25 tests) and `sync_module_works_layout` (9 tests), but **no integration test exercises the supervisor-hook path**.
- **Detail**: The 25 lib tests cover `promote_outline_to_index` (the single-outline promote function) thoroughly (new, idempotent, conflict, allocate, atomic, noop-mtime). But the end-to-end path that the production daemon takes — `on_schedule_terminal` → `promote_foreshadowing_for_schedule` → schedule-row SQL lookup → `works::get_work` → `Outlines/chapters/` directory scan → per-outline `promote_outline_to_index` → file mtime — is not hermetically tested.
  - The schedule-row SQL and Work-lookup logic are not exercised against a real `SqlitePool` for the `promote_foreshadowing_for_schedule` function (only `parse_event_index_reads_populated_table` and the in-process unit tests touch this path).
  - The `promote_outlines_in` directory-walk + per-outline error isolation (warn + continue) is not covered.
  - The `outlines_chapters.is_dir()` short-circuit (no dir → no-op) is not covered.
  - The `Outlines/chapters/` sorting and the early-return on `extract_foreshadowing_section == None` are not covered.
- **Why Suggestion (not Warning)**: The 25 lib tests cover the core `promote_outline_to_index` semantics exhaustively. The supervisor-hook path follows the same pattern as the pre-existing `persist_review_findings_for_schedule` (covered by `tests/review_findings.rs` and `tests/review_report.rs`), so the integration risk is low. The completion report does not register a residual for this gap.
- **Suggested follow-up (V1.49 P2 or V1.50)**: add an integration test under `crates/nexus-orchestration/tests/narrative_index_integration.rs` (or extend `auto_chain.rs` test binary) that:
  1. Seeds a `creator_schedules` row with `preset_id = "novel-writing"` and a `work_id` referencing a Work with a real `workspace_dir`.
  2. Writes 2–3 `Outlines/chapters/*-outline.md` files with mixed `F###:` and bullet forms.
  3. Calls `promote_foreshadowing_for_schedule(pool, "S-1", Some(ws_path))`.
  4. Asserts the result count, asserts `foreshadowing.md` content, and asserts the second call is a no-op (idempotent at the schedule level).
  5. Asserts the per-outline warn-and-continue behavior on a conflicting-description outline.

#### S-5 — Pre-existing flake R-V149P1-02 verified on `integration_merge_target` (not blocking)
- **Where**: `crates/nexus-orchestration/tests/review_report.rs::fallback_warn_includes_chapter_field` (line 622). Listed in `.mstar/status.json` `residual_findings["2026-06-17-v1.49-narrative-indexes"][R-V149P1-02]`.
- **Detail**: The flake reproduces on `iteration/v1.49 @ 946cfba6` (current HEAD) and on `origin/main @ be27111b` (integration_merge_target). Sampling:
  - `iteration/v1.49 @ 946cfba6`: in one in-binary full run, 1 of 3 attempts failed (`fallback_warn_includes_chapter_field` panicked: "expected ≥1 tracing event from the fallback path; got none"). In 5 isolated single-test runs, 5/5 pass.
  - `origin/main @ be27111b` (verification per `.mstar/AGENTS.md` Pre-existing claim verification protocol, step 2): 2 of 10 in-binary runs failed (20% rate). Single-test runs all pass.
- **Verification (Pre-existing claim verification protocol)**:
  1. Failing test identified: `fallback_warn_includes_chapter_field` (tracing subscriber cross-binary race).
  2. Run against `origin/main` (`be27111b`, integration_merge_target) — fails 2/10 in-binary runs.
  3. **Fails on current main → pre-existing claim is TRUE** per step 4 of the protocol.
  4. Not flaky-deterministic: causes vary (different preceding test orderings produce different `tracing::subscriber` global state).
  5. Reproduce command (this reviewer): `cd /Users/bibi/workspace/organizations/42ch/nexus && cargo test -p nexus-orchestration --test review_report` (re-run 3-5 times to observe flake).
- **Why Suggestion (not Warning)**: V1.49 P1 does not modify `tests/review_report.rs` or its capture layer (the test is in the diff's "out-of-scope" zone; diff at `3630a4e5..f448b658` does not touch the file). The flake is pre-existing per the canonical protocol and is properly tracked in `status.json` with `decision: defer`, `target: V1.50`, `severity: low`. The implementer's verification is correct in spirit but their quoted claim "passes with `cargo test -p nexus-orchestration --test review_report` (in isolation, parallel and serial)" is partially misleading — single-test runs always pass, but the full in-binary run flakes ~1-in-3 to ~1-in-5.
- **Suggested follow-up (V1.50, per existing residual R-V149P1-02)**: scoped subscriber guard (per-test) or serializing that single test (`#[serial]` from `serial_test` crate). Document the actual flake rate (~20–30% in-binary) in the residual note for the next reviewer.

## Source Trace
- Finding S-1: source = manual-reasoning + completion.md §Risks/follow-ups; reference `crates/nexus-orchestration/src/auto_chain.rs:1186-1242` (promote_outlines_in directory walk) + `crates/nexus-orchestration/src/schedule/supervisor.rs:459` (hook call); confidence = High
- Finding S-2: source = manual-reasoning + git-diff; reference `crates/nexus-orchestration/src/stage_gates.rs:191-211` (build_preset_input block) + `crates/nexus-orchestration/src/narrative_index.rs:579-597` (read_foreshadowing_summary); confidence = High
- Finding S-3: source = manual-reasoning; reference `crates/nexus-orchestration/src/narrative_index.rs:557-564` (atomic_write) + `Cargo.toml:48` (tempfile dep already present); confidence = High
- Finding S-4: source = manual-reasoning + grep; reference `crates/nexus-orchestration/src/auto_chain.rs:1095-1242` (the unreviewed integration path) + tests/review_report.rs for the analogous `persist_review_findings_for_schedule` pattern; confidence = High
- Finding S-5: source = manual-reasoning + test execution; reference `crates/nexus-orchestration/tests/review_report.rs:622` + `.mstar/status.json` residual_findings["2026-06-17-v1.49-narrative-indexes"][R-V149P1-02] + `.mstar/AGENTS.md` "Pre-existing claim verification protocol" step 4; confidence = High

## Performance / Reliability Dimension — Verification Matrix

| # | Concern (from assignment) | Result |
|---|---------------------------|--------|
| 1 | Hot path: `promote_outline_to_index` on every `novel-writing` terminal event — 50 chapter Work = 50 reads/outline × N outlines = O(N²) over Work lifetime | Acceptable for V1.49 MVP. For N=50: 1275 reads ≈ 130 ms; for N=200: 20 100 reads ≈ 2 s. Implementer correctly defers incremental optimization to V1.50. Tracked as S-1. |
| 2 | `narrative_index.rs` parser/serializer performance — any O(n²) or quadratic behavior? | No. `parse_foreshadowing_index` is O(L) over file lines (single linear pass). `serialize_foreshadowing_index` is O(R) over rows. `next_f_id` is O(R) (single `.max()` pass). `read_foreshadowing_summary` is O(R) (parse + format). All linear; no hidden quadratic loops. For 100 F### rows: <1 ms. For 1000 rows: <10 ms. |
| 3 | `read_foreshadowing_summary` reads the file on every `build_preset_input` call — how often is `build_preset_input` called? | Called once per stage advance (CLI/daemon `stage advance`). For 50 chapters × ~3 stages = ~150 calls/Work. Per-call cost: ~100 µs (read small file + parse + format). Total: ~15 ms per Work. No caching needed for V1.49 MVP. Tracked as S-2. |
| 4 | Resource lifecycle: temp-write + rename. What if rename fails? Temp file cleaned up? Errors logged? | `atomic_write` does **not** clean up the temp file on `fs::rename` failure (no explicit `if let Err = rename { fs::remove_file(tmp) }`). The temp file leaks until the next successful `promote_outline_to_index` call (deterministic name, self-healing). For the single-writer daemon model, the leak window is bounded. Tracked as S-3. Errors are propagated via `?` to the caller (`promote_outline_to_index`) → `promote_outlines_in` logs `warn!` with the per-outline filename. End-to-end error observability is intact. |
| 5 | Sync_module skip invariant: O(1) per file or O(n) per directory walk? | O(1) per file in practice. `SKIP_FILES: &[&str] = &["README.md", "foreshadowing.md", "event-index.md"]` is a 3-element slice; `SKIP_FILES.contains(&fname.as_str())` is a linear scan over 3 elements (O(3) = O(1)). For 10k+ files in `Stories/`, the skip adds <1 µs per file (negligible). The new `sync_module_skips_foreshadowing_index_file` regression test (lines 564–608) locks the invariant at both layers: filename in `SKIP_FILES` AND canonical location in `Outlines/` (which is never scanned). |
| 6 | `on_schedule_terminal` hook fan-out: 100 chapters → 100 hook fires or 1/schedule? | **Once per schedule terminal** (not once per chapter). Each `novel-writing` schedule covers one chapter's full FL-E flow. For 100 chapters: 100 schedules → 100 hook fires. Each fire reads **all** outlines (cumulative cost O(N²) over Work lifetime — see S-1). The hook is best-effort + non-fatal (`tracing::warn!` on error, does not block terminal transition), matching the `persist_review_findings_for_schedule` pattern. |
| 7 | Pre-existing flake R-V149P1-02 — verified on origin/main? | **Yes — verified pre-existing.** Per `.mstar/AGENTS.md` Pre-existing claim verification protocol: ran `cargo test -p nexus-orchestration --test review_report` 10× against `origin/main @ be27111b` (integration_merge_target) in a `git worktree`; 2/10 failed (20% rate). The flake reproduces on `main`, confirming the residual is pre-existing and **not a V1.49 P1 regression**. Tracked as S-5. |
| 8 | CI gates: `cargo +nightly fmt --all --check` and `cargo clippy --all -- -D warnings` | **Both clean.** `+nightly fmt --check` produces no output (clean). `cargo clippy --all -- -D warnings` Finished dev profile in 18.01s with no warnings or errors. (Note: R-V149P0-03 "pre-existing clippy drift" was reported on `iteration/v1.49 @ bc8efc8d`; the current run is clean — either the toolchain is now consistent or the prior drift was resolved by a downstream commit. Verified the drift does not block this P1.) |
| 9 | Test runtime: 25 lib + 22 integration. Any test >1s? Any flaky tests? | All hermetic and fast: 25 lib (0.04s), 22 novel_project_init (2.22s — only one above 1s, still under 3s), 11 e2e_novel_writing (0.02s), 9 sync_module_works_layout (0.01s). The single test >1s (`novel_project_init` at 2.22s) is a 22-test integration binary, so the per-test average is ~100 ms. No timing-dependent logic observed. |
| 10 | Memory profile: any allocations on the hot path that could be reduced? | Acceptable. `read_foreshadowing_summary` returns an owned `String` of < 10 KiB. `parse_foreshadowing_index` allocates a `Vec<ForeshadowingRow>` of owned `String` fields; for 100 rows ≈ 25 KiB, for 1000 rows ≈ 250 KiB. `promote_outline_to_index` reads → parse → mutate → serialize → write; peak memory ≈ 2× the parsed row set. For 100 rows: ~50 KiB. No hidden large allocations or accidental cloning. |

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 5 |

**Verdict**: **Approve**

All 5 Suggestions are V1.49-MVP-acceptable trade-offs or properly-tracked pre-existing residuals. None block the plan from advancing to QA. The implementation is well-tested (25 lib + 22 integration + 11 e2e + 9 sync_module + 3 stage_gates), CI-clean, and the performance/reliability characteristics are appropriate for the local-first, single-writer, single-Work-per-daemon design. The hook placement, atomicity model, and sync-invariant all hold under the documented assumptions.

## Notes for PM
- **De-duplication note**: S-3 (atomic_write temp leak) overlaps thematically with qc1-S-4 (orphaned temp on crash) and qc2-W-2 (deterministic temp name race). My angle is the **resource lifecycle on the rename-failure path** (not the crash path or the multi-writer path) — distinct from both. S-5 (R-V149P1-02) is qc3's verification of the pre-existing claim per the `.mstar/AGENTS.md` protocol; it does not duplicate qc1/qc2 (neither flagged the flake).
- **Residual proposal**: none. The 5 Suggestions are all V1.50 follow-ups, not V1.49 blockers. The two existing residuals (R-V149P1-01 doc reconciliation, R-V149P1-02 pre-existing flake) are properly tracked and have correct severities.
- **Branch state caveat**: working branch advanced from `d78d240b` to `946cfba6` during this review (qc-specialist-2's report commit). The implementation scope `3630a4e5..f448b658` is unchanged. PM may want to fast-forward the Assignment's stated HEAD to current `946cfba6` for the consolidated report, but this does not change the review outcome.
- **Test coverage observation**: S-4 (no integration test for `promote_foreshadowing_for_schedule`) is a low-priority test gap, not a functional gap. The 25 lib tests cover the core `promote_outline_to_index` semantics exhaustively, and the supervisor-hook path follows the well-tested `persist_review_findings_for_schedule` pattern. Recommend a V1.49 P2 or V1.50 add.
- **CI gates both clean**: `+nightly fmt --check` (no diff) and `clippy --all -D warnings` (Finished 18.01s). R-V149P0-03 "clippy drift" does not reproduce on this HEAD.
- **R-V149P1-02 verified pre-existing on `origin/main`**: per the `.mstar/AGENTS.md` Pre-existing claim verification protocol step 4, the flake reproduces on `integration_merge_target` (`main @ be27111b`) with 2/10 rate. The implementer's verification is correct in spirit; the quoted "passes in isolation" claim is partially misleading (single-test runs always pass; in-binary runs flake).
