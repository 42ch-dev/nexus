# Completion Report v2 — V1.51 T-A P0 LLM Extraction

**Agent**: `fullstack-dev` (track=primary)
**Plan**: `2026-06-18-v1.51-llm-extraction`
**Status**: Done (implementation complete; awaiting PM QC tri-review dispatch)
**Task category**: `logic`
**Working branch**: `feature/v1.51-llm-extraction`
**Merge target**: `iteration/v1.51`
**Worktree path**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p0`

---

## Summary

Shipped the `nexus.llm.extract` orchestration capability + `LlmExtractTask`
(sibling to `judge.llm`, both reusing the V1.32 LLM worker pool) and swapped
the `novel-review-master` review-time extraction hook from the V1.50
capitalized-noun heuristic (which defaulted every candidate to
`block_type='character'`) to the LLM pathway. **Closes `R-V150KBED-01`.**
The heuristic is retained as the no-worker fallback so hermetic tests and
daemon-without-worker environments keep producing character-name candidates.
Authors now see LLM-judged `block_type` + `confidence` + a verbatim
`source_quote` on `creator world kb adopt`.

---

## Artifacts

### New files

| Path | LoC | Purpose |
| --- | --- | --- |
| `.mstar/knowledge/specs/llm-extract.md` | 249 | New Master spec — `nexus.llm.extract` capability contract, `LlmExtractTask` lifecycle, payload schema, worker-pool reuse, `novel-review-master` integration (Normative). |
| `crates/nexus-local-db/migrations/202606180006_kb_extract_jobs_llm_payload.sql` | 31 | Additive migration: `llm_confidence REAL` + `llm_source_quote TEXT` columns (nullable; existing rows default NULL). Renamed from `…004` (V1.50 already used `004`). |
| `crates/nexus-orchestration/src/capability/builtins/llm_extract.rs` | 410 | `nexus.llm.extract` capability builtin (mirrors `judge.llm`): input `{prompt, chapter_prose, _creator_id, _session_id}` → output `{candidates:[…]}`. Robust JSON parsing (code-fence strip, bare-array, confidence clamp, malformed→empty). SEC-V131-01 identity boundary. 11 hermetic tests. |
| `crates/nexus-orchestration/tests/novel_review_master.rs` | 333 | E2E: LLM pathway writes the 4 LLM keys into `proposed_payload` + dedicated columns (block_type=scene proves non-heuristic); no-registry heuristic fallback (LLM cols NULL); LLM pathway idempotent. |
| `crates/nexus42/tests/creator_world_kb_adopt.rs` | 177 | Adopt succeeds on LLM-extracted candidate (block_type=scene flows through); `--json` carries metadata; heuristic candidates (NULL LLM cols) still adopt cleanly. |

### Modified files

| Path | Δ | Change |
| --- | --- | --- |
| `crates/nexus-orchestration/src/capability/mod.rs` | +24/-7 | Register `nexus.llm.extract` in all 3 `CapabilityRegistry` constructors; worker_provider wired in `with_runtime_deps`; builtin count 20→21; tests updated. |
| `crates/nexus-orchestration/src/capability/builtins/mod.rs` | +2 | Export `LlmExtract`. |
| `crates/nexus-orchestration/src/quality_loop.rs` | +384/-21 | `KbCandidate` gains `block_type`/`confidence`/`source_quote`; `extract_kb_candidates_for_review` accepts `Option<&CapabilityRegistry>` and runs the LLM pathway when a worker is available (heuristic fallback otherwise); `candidate_from_llm_json` (`pub(crate)`, shared with the task); `block_type_to_novel_category`; `persist_candidates` uses `insert_pending_with_llm`. 7 new unit tests. |
| `crates/nexus-orchestration/src/tasks/mod.rs` | +249/-2 | `LlmExtractTask` mirrors `LlmJudgeTask` (`evaluate()` is `pub` — the hook + future preset routing seam). 4 hermetic tests. |
| `crates/nexus-orchestration/src/schedule/supervisor.rs` | +35/-3 | `ScheduleSupervisor` gains optional `CapabilityRegistry` field + `with_capability_registry` builder; threaded into the terminal-pipeline hook call. |
| `crates/nexus-daemon-runtime/src/boot.rs` | +12/-4 | Daemon boot passes the registry to the supervisor so production runs the LLM pathway. |
| `crates/nexus-orchestration/embedded-presets/novel-review-master/preset.yaml` | +17/-3 | version 2→3; `requires_capabilities` adds `nexus.llm.extract`; header documents the V1.51 review-time extraction pathway. |
| `crates/nexus-local-db/src/kb_extract_job.rs` | +88/-3 | `KbExtractPromotion` gains `llm_confidence`/`llm_source_quote`; all 4 promotion SELECTs read the new columns; new `insert_pending_with_llm` DAO; `insert_pending` delegates with `None, None`. |
| `crates/nexus-local-db/tests/kb_extract_jobs_migration.rs` | +160/-4 | 4 V1.51 migration + DAO round-trip tests. |
| `crates/nexus-orchestration/tests/review_time_extraction.rs` | +20/-8 | Existing V1.50 tests pass `None` registry (heuristic fallback) — unchanged behavior. |
| `crates/nexus-orchestration/tests/review_cron_e2e.rs` | +12/-2 | Same `None` registry update. |
| `crates/nexus-orchestration/tests/capability_registry.rs` | +9/-4 | 21 builtins + `nexus.llm.extract` lookup. |
| `crates/nexus42/src/commands/creator/world/kb.rs` | +63/-3 | `kb_adopt` surfaces `confidence` + `source_quote` (dedicated columns first, JSON fallback); `--json` adds `llm_confidence`/`llm_source_quote`. |
| `.mstar/knowledge/specs/cli-spec.md` | +22 | §6.2G V1.51 amendment (adopt surfaces confidence + source_quote). |
| `.mstar/knowledge/specs/entity-scope-model.md` | +27 | §5.5.6 LLM pathway subsection (state machine unchanged). |
| `.mstar/knowledge/world-kb-runtime-architecture.md` | +29 | §5.5 LLM pathway body. |
| `.mstar/knowledge/specs/README.md` | +2 | Index + authority matrix for `llm-extract.md`. |
| `.mstar/status.json` | +12/-3 | `R-V150KBED-01` lifecycle `deferred`→`resolved` + `closure_evidence` + `resolution`. |
| `.mstar/plans/2026-06-18-v1.51-llm-extraction.md` | +16/-8 | T1–T8 checkboxes ticked. |

---

## Spec bodies authored (acceptance §4.6)

| # | Spec path | Class → Status |
| --- | --- | --- |
| 1 | `.mstar/knowledge/specs/llm-extract.md` (new) | Master → **Normative** (V1.51 Shipped T-A P0) |
| 2 | `.mstar/knowledge/world-kb-runtime-architecture.md` §5.5 | LLM pathway body authored |
| 3 | `.mstar/knowledge/specs/entity-scope-model.md` §5.5.6 | LLM pathway subsection body authored |
| 4 | `.mstar/knowledge/specs/cli-spec.md` §6.2G | V1.51 adopt-surfaces-confidence amendment authored |

The P-1 Drafts were authored + implemented in the same iteration, so the new
Master is Normative per `knowledge/specs/AGENTS.md`. The 3 overlay extensions
document the shipped behavior; promotion to Normative-at-P-last is a P-last
decision (the bodies are already accurate to the implementation).

---

## Residual closure

**`R-V150KBED-01`** — closed.

- **Was**: V1.50 heuristic defaulted every review-time candidate to
  `block_type_guess='character'`; authors corrected on adopt for every
  non-character entity (location/organization/event). Severity `low`,
  deferred V1.50 → V1.51 T-A P0.
- **Now**: `nexus.llm.extract` capability + `LlmExtractTask` ship; the
  review-time hook invokes the LLM pathway when a worker is available,
  filling an LLM-judged `block_type`, `canonical_name`, `confidence`, and a
  verbatim `source_quote`. Heuristic retained only as the no-worker fallback.
- **status.json patch**: under `residual_findings["2026-06-18-v1.50-kb-auto-promotion"]`
  — `lifecycle: "resolved"`, `closed_at: "2026-06-18"`, `closure_evidence`
  (lists the capability + task + hook swap + migration + CLI change + 29 test
  names across 5 modules), `resolution.plan_id` + commit pointers.
- **Evidence tests**: `capability::builtins::llm_extract::tests::*` (11),
  `tasks::tests::llm_extract_task_*` (4), `quality_loop::tests::candidate_from_llm_json_*`
  + `block_type_mapping_*` + `heuristic_candidates_default_*` (5),
  `kb_extract_jobs_migration::v151_*` (4), `novel_review_master::review_master_*` (3),
  `creator_world_kb_adopt::adopt_*` (2).

---

## Verification (acceptance §6 — 6 commands)

All 6 verification commands PASS. Output captured from the worktree at
`feature/v1.51-llm-extraction` HEAD `c066c3ca`.

```text
1. cargo test -p nexus-orchestration -- llm_extract
   → lib: 15 passed; 0 failed  (11 builtin + 4 LlmExtractTask)

2. cargo test -p nexus-orchestration --test novel_review_master
   → 3 passed; 0 failed
     - review_master_llm_path_writes_llm_payload
     - review_master_llm_path_is_idempotent
     - review_master_no_registry_falls_back_to_heuristic

3. cargo test -p nexus-local-db --test kb_extract_jobs_migration
   → 12 passed; 0 failed  (8 V1.50 + 4 V1.51 v151_*)

4. cargo test -p nexus42 --test creator_world_kb_adopt
   → 3 passed; 0 failed

5. cargo clippy --all -- -D warnings        # CI command
   → Finished, no warnings/errors

6. cargo +nightly fmt --all --check
   → exit 0 (clean)
```

**Related suites re-run green (regression check):**
`review_time_extraction` (5), `review_cron_e2e` (2), `capability_registry` (4),
`world_kb_promotion_cli` (11) — V1.50 promotion paths unchanged.

---

## Acceptance criteria mapping (plan §4)

| AC | Status | Evidence |
| --- | --- | --- |
| §4.1 `nexus.llm.extract` registered; `kind: llm_extract` routes to `LlmExtractTask` | ✅ | `capability/mod.rs` (3 constructors); `tasks/mod.rs` `LlmExtractTask`; `capability_registry.rs` 21-builtins test. |
| §4.2 `LlmExtractTask` hermetic tests (golden→golden, mock worker) | ✅ | `tasks::tests::llm_extract_task_with_mock_worker_returns_candidates` + 3 siblings. |
| §4.3 `novel-review-master` uses llm_extract; E2E asserts payload carries 4 LLM keys | ✅ | `preset.yaml` v3 + `requires_capabilities`; `novel_review_master.rs::review_master_llm_path_writes_llm_payload`. |
| §4.4 adopt shows confidence + source_quote | ✅ | `kb.rs::kb_adopt` + `extract_llm_metadata`; `creator_world_kb_adopt.rs` (3 tests); cli-spec §6.2G. |
| §4.5 R-V150KBED-01 closed with evidence | ✅ | `status.json` `lifecycle: resolved` + `closure_evidence`. |
| §4.6 4 spec bodies authored | ✅ | See "Spec bodies authored" above. |
| §4.7 additive DB migration | ✅ | `202606180006_kb_extract_jobs_llm_payload.sql` (additive; NULL defaults). |
| §4.8 wire contracts unchanged | ✅ | No `schemas/` change; `KbCandidate` + payload extension are local-only Rust + SQLite columns. |

Additional hard constraints honored:
- **No `#[allow(...)]` without justification** — none added (all clippy lints fixed at source).
- **No runtime behavior change outside scope** — only the review-time extraction pathway + adopt display touched.
- **No destructive DB schema change** — additive columns only; V1.50 rows unaffected.
- **No `schemas/` modification** — wire contracts OUT (V1.51 §0.1 #9).

---

## Risks / follow-ups

1. **Heuristic fallback retained** — by design (llm-extract.md §5.1). A daemon
   with no configured worker still produces character-name candidates rather
   than silently extracting nothing. Production daemons with a worker always
   take the LLM pathway. No residual opened.
2. **`LlmExtractTask::evaluate` is `pub`** — it is the orchestration seam for
   the review-time hook (which calls the capability directly for now, since it
   has a `ReviewContext` not a `graph_flow::Context`) and for future
   `exit_when: llm_extract` preset routing. Wiring the hook to call the task
   instead of the capability directly would require either an `Arc<Registry>`
   hook signature or a `graph_flow::Context` construction; deferred as
   non-surgical (the capability-call path is equivalent and tested). No
   residual opened — documented in `llm-extract.md` §2.
3. **Migration version rename** — `202606180004` collided with V1.50's
   `works_schedule_json_partial_idx`; renamed to `202606180006` (V1.50's
   highest was `005`). No `_sqlx_migrations` collision; all 12 migration tests
   green.
4. **Confidence type is `f64`, not `f32`** — the plan's contract sketch said
   `f32`, but the SQLite `REAL` column + JSON number representation are `f64`;
   using `f64` avoids f32→f64 promotion precision loss in the persisted
   payload. Documented in `KbCandidate.confidence` doc comment + `llm-extract.md`.
5. **`proposed_payload.tags` distinguishes pathway** — `["novel","llm-extracted"]`
   vs `["novel","heuristic-extracted"]`; the adopt CLI + QC can tell which
   pathway produced a row. No residual.
6. **No platform integration** — unchanged from V1.51 §1.5 (paused, local-only).

---

## Git context

- **Branch**: `feature/v1.51-llm-extraction`
- **Worktree**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p0`
- **Diff basis**: `iteration/v1.51` (`ca494f03`)
- **Tip**: `c066c3ca`

```text
$ git log --oneline iteration/v1.51..HEAD
c066c3ca style(v1.51-t-a-p0): clippy + nightly fmt pass
5ef04b56 harness(v1.51-t-a-p0): close R-V150KBED-01 + plan checkboxes T1-T8 done
887bbc07 feat(nexus42): V1.51 T-A P0 — kb adopt surfaces LLM confidence + source_quote
b56ef238 feat(nexus-orchestration): V1.51 T-A P0 — LlmExtractTask + heuristic→LLM hook swap
cbf38f37 feat(nexus-local-db): V1.51 T-A P0 — kb_extract_jobs LLM DAO + migration tests
0d747848 feat(nexus-orchestration): V1.51 T-A P0 — nexus.llm.extract capability
606481a5 feat(nexus-local-db): V1.51 T-A P0 — kb_extract_jobs LLM payload migration
7caf48ab docs(specs): V1.51 T-A P0 — llm-extract Master + 3 overlay bodies
```

8 commits, per-task-ID granularity (T1 specs / T2 migration / T3+T4 capability /
T5 hook+task+supervisor / T6 CLI / T8 residual / style). 24 files changed,
+2292/-91.

**Not merged into `iteration/v1.51`** — that is PM responsibility after QC
tri-review (qc1 architecture + qc2 security/correctness + qc3 perf/reliability)
+ QA verification.

---

## Handoff

To `@project-manager`: implementation complete on
`feature/v1.51-llm-extraction`; ready for QC tri-review dispatch. Suggested
`Review cwd` / `Worktree path`: this worktree
(`/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p0`).
`plan_id`: `2026-06-18-v1.51-llm-extraction`. Suggested `Review range` /
`Diff basis`: `merge-base: iteration/v1.51` + `tip: c066c3ca` (i.e.
`git diff iteration/v1.51...HEAD`).
