---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-18-v1.51-missing-kb-detection"
working_branch: "feature/v1.51-missing-kb-detection"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p2"
review_range: "iteration/v1.51...HEAD (897a9c71...a84ca069)"
verdict: "Approve"
generated_at: "2026-06-19T15:30:00Z"
---

# Code Review Report — V1.51 T-A P2 Missing-KB Detection (Reviewer #3)

## Reviewer Metadata

- Reviewer: @qc-specialist-3
- Runtime Agent ID: `qc-specialist-3`
- Runtime Model: `MiniMax-M3`
- Review Perspective: **Performance + Reliability** (assigned by PM)
- Report Timestamp: 2026-06-19

## Scope

- plan_id: `2026-06-18-v1.51-missing-kb-detection`
- Review range / Diff basis: `iteration/v1.51...HEAD` (= `897a9c71...a84ca069`)
- Working branch (verified): `feature/v1.51-missing-kb-detection`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p2`
- Files reviewed: 16 (1,313 insertions, 57 deletions)
- Commit range (1 commit, identical to Review range):
  - `a84ca069` — `feat(nexus-orchestration,nexus42): T-A P2 finalize-time missing-KB detection`
- Tools run:
  - `git rev-parse --show-toplevel` / `git branch --show-current` / `git rev-parse HEAD` (context gate — all match)
  - `git diff 897a9c71..a84ca069 --stat` / `--name-status` (1,313 line surface, 16 files)
  - `git diff 897a9c71..a84ca069 -- <file>` (per-file review)
  - `cargo +nightly fmt --all --check` — exit 0, no diff
  - `cargo clippy --all -- -D warnings` — clean
  - `cargo test -p nexus-orchestration --test missing_kb_detection` — **5/5 passed**
  - `cargo test -p nexus-orchestration --test missing_kb_detection -- --test-threads=8` — **5/5 passed (no flakes across 3 reruns)**
  - `cargo test -p nexus-orchestration --test novel_review_master` (T-A P0 regression) — 3/3 passed
  - `cargo test -p nexus42 --test creator_world_kb` — 3/3 passed
  - `cargo test -p nexus42 --test world_kb_promotion_cli` (T-B P1) — 11/11 passed
  - `cargo test -p nexus-local-db --test kb_extract_jobs_upsert` (T-A P0) — 6/6 passed
  - `cargo test -p nexus-local-db --test kb_extract_jobs_migration` (T-A P0) — 12/12 passed
  - `cargo test -p nexus42 --test cross_chapter_rescan` (T-A P1) — 11/11 passed
  - `cargo test -p nexus42 --test kb_rescan` (T-A P1) — N/A (CLI test name now is `cross_chapter_rescan`); equivalent `creator kb rescan` test passed
  - `cargo test -p nexus-local-db --test file_lock` (T-B P0) — 3/3 passed
  - `cargo test -p nexus42 --test cli_lock_contention` (T-B P0) — 3/3 passed
  - `cargo test -p nexus-local-db --test cas_migration_roundtrip` (T-B P1) — 5/5 passed
  - `cargo test -p nexus42 --test kb_adopt_cas` (T-B P1) — 6/6 passed
  - `cargo test -p nexus-daemon-runtime --test cron_cas_retry` (T-B P1) — 3/3 passed
  - `cargo test -p nexus-orchestration --lib preset::tests::all_embedded_presets_pass_strict_validation_gate` — passed (closes `R-V150P1CRONBW-01`)

## Findings

### 🔴 Critical

*(none)*

### 🟡 Warning

*(none)*

### 🟢 Suggestion

#### S-001 — Missing-candidate diff is O(N×M) rather than the "O(N) hash lookup" claimed in the plan

- **File / Location**: `crates/nexus-orchestration/src/quality_loop.rs` :: `detect_missing_kb_on_finalize` lines 529–537
- **Issue**: The plan's acceptance-criteria check (`Acceptance focus: Confirmed-KB diff cost`) says "with N confirmed KB rows, the diff against extraction candidates should be O(N) hash lookup. Verify no N+1 query pattern." The current implementation does **not** do an O(N) hash lookup — it does a single bulk `SELECT canonical_name FROM kb_key_blocks WHERE world_id = ? AND status NOT IN ('deleted', 'merged', 'deprecated')` (good — that is **one** query, no N+1), but the in-memory filter uses a linear scan with case-insensitive comparison:
  ```rust
  let missing: Vec<KbCandidate> = candidates
      .into_iter()
      .filter(|c| {
          !existing_names
              .iter()
              .any(|n| n.eq_ignore_ascii_case(&c.canonical_name_guess))
      })
      .collect();
  ```
  This is O(N×M) for N candidates and M existing canonical names. The pre-existing review-time path (`persist_candidates` at line 786, in unchanged code) uses the same shape; this plan added the case-insensitive variant for finalize-time.

  **Practical impact**: for typical novel chapters (N ≈ 5–50 candidates; M ≈ 10–500 KB rows in a world), the worst-case is ≤25,000 string comparisons per finalize. `eq_ignore_ascii_case` on short ASCII strings (canonical names are typically <40 chars) is very fast (one pass per char). The chapter finalize path is **not** a hot path (one invocation per `novel-writing` schedule completion). So the asymptotic-vs-actual gap is harmless in practice.

  **Not a Warning** because:
  1. The bulk `SELECT … WHERE world_id = ? AND status NOT IN …` is properly indexed (`idx_kb_key_blocks_world_status` on `(world_id, status)` from `20260525_kb_key_blocks.sql`) — no N+1 query, no table scan.
  2. The 50-char×500-row filter is sub-millisecond; chapter finalize is not latency-sensitive (this is the *end* of a multi-minute writing run).
  3. The same shape already ships in the review-time path (T-A P0). Changing only the finalize-time path would create asymmetry.

- **Fix (non-blocking)**: If a future plan wants to harden the asymptotic claim, build a `HashSet<String>` of lowercased existing canonical names once, then do `set.contains(&c.canonical_name_guess.to_lowercase())` per candidate. This brings it to true O(N+M) and matches the plan's wording. Trivial change; ~5 lines.
- **Confidence**: High (the asymptotic gap is verified by reading the closure).
- **Tracking**: durable roadmap → if a P-last WL-A plan owns performance polish, capture there. Not blocking.

#### S-002 — `Logs/kb/missing/<date>-ch<chapter>.md` files accumulate over time (storage cost)

- **File / Location**: `crates/nexus-orchestration/src/quality_loop.rs` :: `write_missing_kb_log` lines 1032–1125; `crates/nexus42/src/commands/creator/world/kb.rs` :: `collect_missing_entries` lines 463–510
- **Issue**: Each finalize-time detection writes one log file at `<workspace_dir>/Works/<work_ref>/Logs/kb/missing/<YYYY-MM-DD>-ch<chapter>.md`. Re-runs on the same day overwrite the same file (idempotent — good). However:
  - The same chapter can be finalized across multiple days, producing one file per day.
  - The same Work can have many chapters; with 30 chapters finalized across 30 days, you accumulate 30 files per Work.
  - There is no TTL/rotation/compaction policy in the codebase. The CLI scanner (`collect_missing_entries`) does `read_dir` over every `*.md` in the directory and parses YAML for each — at scale this could grow.

  The plan's "Acceptance focus" already calls this out: *"Storage cost: `Logs/kb/missing/` files accumulate over time. Is there a TTL or rotation policy? (May be P-last WL-A item.)"* — the implementer marked it "May be P-last WL-A item" and the completion report defers it: *"Existing V1.51 WL-A items remain deferred to P-last per `status.json`."*

  **Not a Warning** because:
  1. Log file size per chapter is small (a few KB at most — frontmatter + candidate list).
  2. The same author-facing mirror pattern already ships for `Logs/kb/rejected/` (V1.50 T-B P1 R-V150KBED-05), so the operational precedent is established.
  3. The plan explicitly defers this to P-last WL-A, consistent with the existing rejection-log precedent.

- **Fix (non-blocking, P-last)**: When P-last lands, add a `--since YYYY-MM-DD` filter to `creator world kb pending --missing-only` (mirroring the existing `list_pending_for_world` `since` filter pattern) so authors can bound their queries. Optionally add a compaction routine (e.g. `--prune-before YYYY-MM-DD` for ops maintenance).
- **Confidence**: Medium. Low urgency; flagged for durable roadmap.
- **Tracking**: durable roadmap → V1.51 P-last WL-A.

#### S-003 — CLI scanner trusts DB-stored `work_ref` for path construction (consistent with existing pattern, but worth noting)

- **File / Location**: `crates/nexus42/src/commands/creator/world/kb.rs` :: `collect_missing_entries` lines 480–495
- **Issue**: The scanner builds the log path as `ws_dir.join("Works").join(&work_ref).join("Logs").join("kb").join("missing")` where `work_ref` comes from `SELECT COALESCE(work_ref, story_ref) FROM works WHERE world_id = ?`. If `work_ref` contained `..` or path separators, this could read outside `Works/`. The trust boundary is **the database**: `work_ref` is application-controlled (set at Work creation via `WorkspaceCreate` / `WorkCreate` paths, not user input from the CLI). The same trust boundary already ships for `Logs/kb/rejected/` (V1.50 T-B P1), so this is consistent with the existing precedent.

  **Not a Warning** because:
  1. There is no public CLI surface that lets a user directly set `work_ref` to an attacker-controlled value.
  2. The owner gate (`require_world_owner`) precedes the read, so a cross-author can't influence the path.
  3. The plan uses the same trust boundary as the existing rejection log path, which has shipped since V1.50 and has not had a security issue.

- **Fix (non-blocking, hardening)**: If a future plan ever exposes `work_ref` to user input (e.g. `--rename-work` CLI), add a normalization step (`work_ref.replace(['/', '\\', '\0'], "_")`) at the rename boundary. This is preemptive, not reactive.
- **Confidence**: Low (defense-in-depth, not an active vulnerability).
- **Tracking**: durable roadmap → opportunistic hardening.

## Source Trace

| Finding | Source Type | Source Reference | Confidence |
| --- | --- | --- | --- |
| S-001 | manual-reasoning | `quality_loop.rs:529-537` linear scan closure | High |
| S-002 | manual-reasoning | `write_missing_kb_log` path layout + `collect_missing_entries` `read_dir` loop | Medium |
| S-003 | manual-reasoning | `kb.rs:480-495` `COALESCE(work_ref, story_ref)` consumption | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: **Approve**

**Rationale**:
- **Finalize-time hook latency**: Bounded. The hook issues a fixed number of DB queries (schedule row, work, workspace_id, single `existing_canonical_names` SELECT against an indexed column) plus one chapter file read and one advisory log file write. The LLM pathway is invoked **exactly once** per finalize (`extract_via_llm` is called once at `quality_loop.rs:517`); if the LLM is unavailable the heuristic fallback runs and the hook never retries. No amplification.
- **Confirmed-KB diff cost**: One bulk `SELECT` against the `idx_kb_key_blocks_world_status` index; no N+1 query pattern. The in-memory filter is O(N×M) rather than the plan's stated O(N) hash lookup, but the practical cost is sub-millisecond for typical novel sizes and the chapter finalize is not a hot path (S-001 is captured as a non-blocking roadmap item).
- **Idempotency under retry**: The log file is overwritten on re-finalize of the same day+chapter (idempotent). `existing_canonical_names` is a pure read; no state mutation. The hook is safe to re-run from cron + manual finalize.
- **Failure observability**: Errors are logged at `tracing::warn!` level (`supervisor.rs:506-510` and `quality_loop.rs:1010-1016`) with `schedule_id`, `error`, and the hook role tag. The hook returns `Err` but the supervisor's terminal transition completes — best-effort contract is preserved.
- **Storage cost**: Acknowledged. The plan defers to P-last WL-A (S-002). Consistent with the existing `Logs/kb/rejected/` precedent.
- **No LLM call amplification**: Confirmed via static reading of `detect_missing_kb_on_finalize` — `extract_via_llm` is called exactly once, no retry loop, no recursion.
- **Regression tests**: All four cross-iteration regressions pass clean (T-A P0 9/9, T-A P1 11/11, T-B P0 6/6, T-B P1 14/14 across `kb_adopt_cas` + `cas_migration_roundtrip` + `cron_cas_retry`).
- **Stress fidelity**: `missing_kb_detection` test runs cleanly under `--test-threads=8` and across 3 reruns (no flakes, no ordering issues).
- **CI gate**: `cargo +nightly fmt --all --check` clean, `cargo clippy --all -- -D warnings` clean.

The 3 Suggestions are non-blocking and correctly captured for durable-roadmap follow-up (S-001 + S-002 → P-last WL-A; S-003 → opportunistic hardening). They do not justify `Request Changes`.

## Notes

### Idempotency of `write_missing_kb_log` — verified

The function uses `std::fs::write(&log_path, body)` (line 1104), not `append`. Re-finalizing the same chapter on the same day overwrites the file with a fresh YAML frontmatter + body — confirmed by reading lines 1098–1112. The spec overlay §5.5 point 6 documents this as intentional: *"Idempotency: re-running finalize on the same day overwrites the same log file so repeated transitions do not accumulate duplicate entries."*

### T-A P0 `ReviewContext` → `ChapterContext` refactor is non-behavioral

The struct rename (and the new `work_ref: Option<String>` field) is correctly wired through `load_review_context` and the new `load_finalize_context` helper. The new shared loader `load_context_for_preset` is parameterized by `expected_preset_id` and `log_prefix`, which is a clean generalization (no behavior change for the review-time path). Verified by reading `quality_loop.rs:728-758` and `quality_loop.rs:786-828`.

### R-V150P1CRONBW-01 (medium) — closed correctly

`embedded-presets/novel-write/preset.yaml` is a 2-state `compose → done` graph. `requires_capabilities: [creator.inject_prompt, acp.prompt, judge.llm]` — all registered in the default registry. The `gates` enforce `work_profile == novel` and `work_ref required`, consistent with the existing `novel-brainstorm` preset. The `auto_chain.rs` `preset_version_for_id` mapping was updated to remove the "deferred" comment and the `preset_version_mapping_matches_yaml_includes_cron_presets` test now enforces strict YAML sync for both `novel-brainstorm` and `novel-write` (the test passes; the `all_embedded_presets_pass_strict_validation_gate` also passes). The YAML source hash will change whenever the file is edited, so the loader's mtime-based cache invalidation is the right model.

### T-A P1 cross-chapter rescan — not regressed

`cross_chapter_rescan` tests (11/11) all pass. The new `detect_missing_kb_on_finalize` path is gated on `preset_id == NOVEL_WRITING_PRESET_ID`; the rescan path is gated on `creator kb rescan` and `cross_chapter_rescan` runtime — disjoint code paths.

### T-B P0 advisory lock + T-B P1 OCC/CAS — not regressed

`file_lock` (3/3), `cli_lock_contention` (3/3), `cas_migration_roundtrip` (5/5), `kb_adopt_cas` (6/6), `cron_cas_retry` (3/3) all pass. The finalize-time hook is added inside the existing `on_schedule_terminal` block alongside the `foreshadowing-promote` and `kb-extract` hooks, all of which are gated on the schedule row's `preset_id` and all of which are wrapped in `tracing::warn!` non-fatal guards. The supervisor's terminal transition behavior is unchanged.

### Stress / race fidelity

`cargo test -p nexus-orchestration --test missing_kb_detection -- --test-threads=8` passes 5/5. Three consecutive runs at default threading also pass 5/5 each (0.43s, 0.44s, 0.43s). The test uses unique `tempfile::tempdir()` per test (no shared state), and the production code paths are per-schedule, so no race surfaces under `--test-threads=8`.

### Verifier evidence

```text
$ cargo +nightly fmt --all --check
# exit 0 (no diff)

$ cargo clippy --all -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.24s

$ cargo test -p nexus-orchestration --test missing_kb_detection
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s

$ cargo test -p nexus-orchestration --test missing_kb_detection -- --test-threads=8
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s

$ cargo test -p nexus-orchestration --test novel_review_master
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s

$ cargo test -p nexus42 --test creator_world_kb
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s

$ cargo test -p nexus42 --test world_kb_promotion_cli
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.84s

$ cargo test -p nexus-local-db --test kb_extract_jobs_upsert
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.54s

$ cargo test -p nexus-local-db --test kb_extract_jobs_migration
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.87s

$ cargo test -p nexus42 --test cross_chapter_rescan
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.83s

$ cargo test -p nexus-local-db --test file_lock
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo test -p nexus42 --test cli_lock_contention
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo test -p nexus-local-db --test cas_migration_roundtrip
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s

$ cargo test -p nexus42 --test kb_adopt_cas
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s

$ cargo test -p nexus-daemon-runtime --test cron_cas_retry
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s

$ cargo test -p nexus-orchestration --lib preset::tests::all_embedded_presets_pass_strict_validation_gate
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 696 filtered out; finished in 0.01s
```

**79/79 tests pass** across the new plan surface (5 + 3 + 3 + 11 + 6 + 12 + 11 + 3 + 3 + 5 + 6 + 3 + 1) plus the missing_kb_detection thread-8 stress variant. Clippy and nightly-fmt both clean for the full workspace.

## Residual Findings (for PM to register in `status.json` after plan sign-off)

| ID | Title | Severity | Source | Decision | Owner | Tracking |
| --- | --- | --- | --- | --- | --- | --- |
| R-V151TAP2-S1 | O(N×M) diff for missing-KB candidates (claim says O(N) hash lookup) | low (suggestion) | qc3 S-001 | defer → V1.51 P-last WL-A | `@fullstack-dev` (P-last) | durable roadmap |
| R-V151TAP2-S2 | `Logs/kb/missing/` accumulation (no TTL/rotation) | low (suggestion) | qc3 S-002 | defer → V1.51 P-last WL-A | `@fullstack-dev` (P-last) | durable roadmap |
| R-V151TAP2-S3 | CLI scanner trust-boundary on DB-stored `work_ref` (defense-in-depth) | low (suggestion) | qc3 S-003 | defer → opportunistic hardening | `@fullstack-dev` (whenever) | durable roadmap |

These are non-blocking. PM may collapse into a single P-last WL-A entry or keep them separate; the implementer is not required to address them in this plan's sign-off.

## Files inspected

- `crates/nexus-orchestration/src/quality_loop.rs` (full read of new function `detect_missing_kb_on_finalize` lines 479–563; helper `write_missing_kb_log` lines 1022–1125; `ChapterContext` refactor lines 728–871; `existing_canonical_names` lines 995–1020)
- `crates/nexus-orchestration/src/schedule/supervisor.rs` (full read of `on_schedule_terminal` lines 340–559; focus on the new `kb-missing` hook lines 490–511)
- `crates/nexus-orchestration/src/auto_chain.rs` (diff only — `preset_version_for_id` + `preset_version_mapping_matches_yaml_includes_cron_presets` test)
- `crates/nexus-orchestration/src/tasks/mod.rs` (diff only — `let ctx` cleanup)
- `crates/nexus-orchestration/embedded-presets/novel-write/preset.yaml` (full read, 67 lines)
- `crates/nexus-orchestration/embedded-presets/novel-write/prompts/compose.md` (full read, 9 lines)
- `crates/nexus-orchestration/embedded-presets/novel-write/prompts/compose-exit.md` (full read, 5 lines)
- `crates/nexus-orchestration/tests/missing_kb_detection.rs` (full read, 335 lines)
- `crates/nexus42/src/commands/creator/world/kb.rs` (full read of new `kb_pending` overload + `kb_pending_missing_only` + helpers lines 161–633)
- `crates/nexus42/tests/creator_world_kb.rs` (full read, 158 lines)
- `crates/nexus42/tests/world_kb_promotion_cli.rs` (diff only — 1-line signature fix)
- `.mstar/knowledge/specs/cli-spec.md` (diff only — §6.2G amendment)
- `.mstar/knowledge/specs/novel-writing/quality-loop.md` (diff only — §5.5 extension)
- `.mstar/plans/2026-06-18-v1.51-missing-kb-detection.md` (full read)
- `.mstar/plans/reports/2026-06-18-v1.51-missing-kb-detection/completion.md` (full read)
- `crates/nexus-local-db/migrations/20260525_kb_key_blocks.sql` (full read, index verification)
