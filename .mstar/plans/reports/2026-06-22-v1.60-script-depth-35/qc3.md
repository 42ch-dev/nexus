---
report_kind: qc
reviewer: qc-specialist-3 (Reviewer #3 — performance/reliability)
reviewer_index: 3
plan_id: "2026-06-22-v1.60-script-depth-35"
verdict: "Approve"
generated_at: "2026-06-23"
---

## QC3 Review — Performance & Reliability (Reviewer #3)

**Plan**: V1.60 P1 — Script Profile Depth 3.5 Promotion (Draft → Master)
**Reviewer**: qc-specialist-3 (performance/reliability focus)
**Review range**: `7cec348d..4d322c7c`
**Commit 4d322c7c**: `fix(v1.60-pmid): update hardcoded registry count 26→31 for 5 new V1.60 P0 capabilities`

---

## Summary

Script profile promoted from Draft scaffold (V1.55 P3) to Master Depth 3.5 with: embedded preset `script-writing` (outline→draft→revise→finalize chain + 五问 quality gate), section completion detection (`is_script_complete` with 2-file async read), profile-aware KB extraction (`candidate_from_llm_json_for_profile` script branch), DB migration for `work_profile = 'script'`. **Verdict: Approve** — no performance or reliability blockers.

---

## Findings

### P1 Track B (Script Depth 3.5)

#### F1 — Preset load performance — O(N) linear scan acceptable (N - Note)
**Location**: `crates/nexus-orchestration/embedded-presets/script-writing/preset.yaml` (131 lines)
**Severity**: N (Note)

Preset YAML is small (131 lines). Loader validation checks are O(N) over YAML nodes (existing loader logic in `loader.rs`, not changed in V1.60). Validation gates: `requires_capabilities` (3 entries), state graph (5 states, cycle-free), `work_profile = 'script'` gate (O(1)). No O(N²) patterns.

**Observation**: Acceptable linear validation for small preset size. Loader logic is shared with all presets; no new performance regressions.

---

#### F2 — Section completion check — efficient query with index (N - Note)
**Location**: `crates/nexus-local-db/src/work_chapters.rs:1427-1515` (`is_script_complete`)
**Severity**: N (Note)

`is_script_complete` queries `works` table by `work_id` (primary key, index-backed). Early exit on `intake_status != 'complete'` (line 1443). Then reads 2 critical files via `tokio::fs::read_to_string`:
- `Works/<work_ref>/Scripts/script.md`
- `Works/<work_ref>/Beats/beat-sheet.md`

FS reads are async (`tokio::fs`), not blocking `std::fs`. Per-section evaluation logs at `debug` level (line 1466); first non-accepted section logs at `info` (line 1451). No full-table scans.

**Observation**: Efficient PK lookup + bounded async file reads (2 files max). Completion check is called from daemon handler layer (not hot path in capability dispatch), so latency is acceptable.

---

#### F3 — KB extraction allocation patterns — efficient, mirrors game-bible (N - Note)
**Location**: `crates/nexus-orchestration/src/quality_loop.rs:772-790` (`candidate_from_llm_json_for_profile` script branch)
**Severity**: N (Note)

Script profile branch (line 772-790) mirrors game-bible branch (line 765-768). Allocation pattern: constructs `proposed_payload` JSON with 4 fields:
- `attributes.script_category` (mapped via `block_type_to_script_category`)
- `block_type`
- `canonical_name`
- `tags` (fixed array `["script", "llm-extracted"]`)

No unbounded allocation. `block_type_to_script_category` is a pure function with `match` arms (no dynamic allocation).

**Observation**: Efficient allocation. Reuses proven game-bible pattern.

---

#### F4 — Preset version SSOT lookup — O(1) HashMap match (N - Note)
**Location**: Per P1 T6 plan, `preset_version_for_id` SSOT in `preset_ids.rs` (extension only, not visible in diff)
**Severity**: N (Note)

Compass §0.1 Q6 confirms `preset_version_for_id` uses a `HashMap` for O(1) lookup. Script-writing addition is a constant-time map entry insert + lookup. The sync test `preset_version_mapping_matches_yaml_includes_cron_presets` validates SSOT integrity (not directly visible in diff, per plan).

**Observation**: O(1) SSOT lookup via HashMap. No linear scan over all presets.

---

#### F5 — Migration performance — CHECK constraint eval cost low (N - Note)
**Location**: `crates/nexus-local-db/migrations/202606230001_work_profile_script.sql`
**Severity**: N (Note)

Migration recreates `works` table with expanded CHECK constraint (line 27):
```sql
CHECK (work_profile IS NULL OR work_profile IN ('novel', 'essay', 'game_bible', 'script'))
```
Constraint eval cost is O(1) per INSERT (4-element enum). Migration steps: create `works_new` (line 9), copy data (line 48), drop old (line 51), rename (line 54), recreate 9 indexes (lines 57-82). All operations are one-time schema change; not in hot path.

**Observation**: Low-cost CHECK constraint. Migration is acceptable for local-first use case (scales linearly with row count, not query complexity).

---

#### F6 — Tracing — script operations have appropriate tracing (N - Note)
**Location**: `work_chapters.rs:1427-1515` (`is_script_complete`), `quality_loop.rs:890-909` (`block_type_to_script_category`)
**Severity**: N (Note)

`is_script_complete` logs:
- `info` for intake_status gate (line 1443)
- `info` for first non-accepted section (line 1451)
- `info` for completion success (line 1470)
- `debug` for per-section evaluation (line 1466)
- `warn` for work not found (line 1438)
- `debug` for work_ref NULL (line 1449)

`block_type_to_script_category` logs `debug!` for unknown block_type fallback to `dialogue` (line 908). All log levels are appropriate (`info` for production gates, `debug` for details).

**Observation**: Full tracing coverage for production paths. No missing observability.

---

#### F7 — Script-writing preset 五问 quality gate — no performance regression (N - Note)
**Location**: `crates/nexus-orchestration/embedded-presets/script-writing/preset.yaml:69-73, 89-93, 107-111, 124-128`
**Severity**: N (Note)

All states (outline, draft, revise, finalize) use the same `exit_when` judgment with `template_file: prompts/finalize-exit.md` and `judge_capability: judge.llm`. This mirrors the game-bible 五问 pattern (V1.55 P2). Judgment is a single LLM call per state; no repeated loops in the preset definition (LLM decides whether to loop to previous state, not the preset).

**Observation**: No performance regression vs game-bible 五问 pattern. Acceptable LLM call latency for local-first use case.

---

#### F8 — Async I/O — correct use of tokio::fs, no blocking (N - Note)
**Location**: `work_chapters.rs:1455` (`tokio::fs::read_to_string`)
**Severity**: N (Note)

`is_script_complete` uses `tokio::fs::read_to_string` for async file reads. No `std::fs` blocking calls in async paths.

**Observation**: Correct async/await usage.

---

#### F9 — Memory — no unbounded allocations (N - Note)
**Location**: All P1 changes
**Severity**: N (Note)

- `is_script_complete`: reads 2 files into `String` (bounded by file size; no script-size limits in V1.60, but typical scripts are <1 MB).
- `candidate_from_llm_json_for_profile` script branch: allocates fixed JSON structure (no unbounded vectors).
- Preset YAML: small (131 lines).

**Observation**: No unbounded memory growth. Typical script file sizes are bounded by user content, not by code.

---

#### F10 — Test fidelity — hermetic tests use real DB + filesystem (N - Note)
**Location**: `work_chapters.rs:2930-3093` (`test_is_script_complete_*`), `quality_loop.rs:1960-2052` (`candidate_from_llm_json_for_profile_script_*`, `block_type_to_script_category_*`)
**Severity**: N (Note)

Script completion tests:
- `test_is_script_complete_all_accepted`: fresh pool, seed work, write 2 files with `section_status: accepted`, assert `is_script_complete` returns `true`.
- `test_is_script_complete_one_draft`: same setup but beat-sheet is `draft`, assert returns `false`.
- `test_is_script_complete_missing_files`: same setup but files missing, assert returns `false`.

KB extraction tests: 5 tests for `candidate_from_llm_json_for_profile` script branch (direct mapping, cross-domain mapping, unknown fallback) + 5 tests for `block_type_to_script_category` (all match arms + cross-domain + unknown).

All tests use real DB + filesystem (no mocks). High test fidelity.

**Observation**: Sufficient hermetic test coverage. No `#[ignore]` tests.

---

### Cross-cutting

#### F11 — Observability — all script operations have tracing (N - Note)
**Location**: `work_chapters.rs`, `quality_loop.rs`
**Severity**: N (Note)

Script completion check has full `info`/`debug` tracing (see F6). KB extraction `block_type_to_script_category` logs `debug!` for unknown block_type. No missing observability in production paths.

**Observation**: Full tracing coverage for script profile operations.

---

#### F12 — Resource limits — no new unbounded resources (N - Note)
**Location**: All P1 changes
**Severity**: N (Note)

- Script completion: reads bounded 2 files.
- KB extraction: bounded per-LLM-response allocation.
- Preset execution: bounded state graph (5 states).

No unbounded file counts, event counts, or loop iterations introduced.

**Observation**: Acceptable resource bounds for local-first use case.

---

#### F13 — Regression risk — low for existing dispatch paths (N - Note)
**Location**: `quality_loop.rs:772-790` (new `else if` branch), `work_chapters.rs:1427-1515` (new function), `migrations/202606230001_work_profile_script.sql` (one-time schema change)
**Severity**: N (Note)

P1 changes are additive:
- New `else if work_profile == "script"` branch in `candidate_from_llm_json_for_profile` (no changes to novel/game_bible branches).
- New `is_script_complete` function (called from daemon handler, not from hot path capability dispatch).
- DB migration: one-time table recreation + index rebuild.

No changes to hot path loops in existing code.

**Observation**: Low regression risk — additive changes only.

---

## Verdict

**Approve**

No Critical or Warning-level performance/reliability issues. All findings are Notes (F1-F13). The O(N) preset validation (F1) is acceptable for small preset size; section completion check (F2) uses efficient PK lookup + bounded async file reads. KB extraction (F3) reuses proven game-bible allocation pattern. No unbounded resources or blocking I/O.

---

## Test Coverage

P1 hermetic tests: 3 script completion tests + 10 KB extraction tests = 13 total. All use fresh DB pool + filesystem writes. High test fidelity — no mocks. All tests pass per plan verification.

**Observation**: Sufficient test coverage for V1.60 P1 delivery. No `#[ignore]` tests.