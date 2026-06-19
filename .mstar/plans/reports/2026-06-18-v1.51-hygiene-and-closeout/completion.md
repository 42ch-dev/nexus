# Completion Report — V1.51 P-last Hygiene & Closeout

**Plan:** `.mstar/plans/2026-06-18-v1.51-hygiene-and-closeout.md`  
**Branch:** `feature/v1.51-hygiene-and-closeout`  
**Integration branch:** `iteration/v1.51`  
**Final PR to `main`:** [#64](https://github.com/42ch-dev/nexus/pull/64)  
**Merged topic PRs:** [#65](https://github.com/42ch-dev/nexus/pull/65) (P-last body), [#66](https://github.com/42ch-dev/nexus/pull/66) (ship metadata follow-up)  
**Date:** 2026-06-19

---

## Summary

P-last closed the V1.51 iteration. Work was split into two topic PRs against `iteration/v1.51`; the integration branch is now ready for final review via PR #64 to `main`.

### Commits (feature branch)

1. `style(nexus-orchestration): WL-A R-V151Q1-02 — doc_markdown backticks for LlmExtractTask test docs`
2. `style(nexus-daemon-runtime): WL-A R-V151Q1-08 — replace deprecated TempDir::into_path with keep(); remove unused imports`
3. `docs(nexus-orchestration): WL-A cron-brainstorm-write/qc1 S-002 — remove stale COORDINATE-WITH-T-A-P2 marker and update hook comment tense`
4. `style(nexus42): WL-A kb-editor-cli/qc2 S-V150KBED-QC2-02 — use 'load' verb for pre-check get_key_block errors in edit/delete`
5. `style(nexus-orchestration): WL-A cron-review-staggering/qc2 — improve idempotency test label + doc_markdown backticks in cron_supervisor tests`
6. `style(nexus-orchestration): WL-A auto-chronology/qc3 S-3 — clippy doc_markdown + match_wildcard_for_single_variants in auto_chronology_tick tests`
7. `style(nexus-local-db): WL-A cron-foundation/qc1 S5 — doc_markdown backticks in works_schedule_migration test docs`
8. `docs(nexus-local-db): WL-A R-V151Q1-09 — document rationale for not marking lock-metadata parse failure as stale`
9. `docs(specs): promote 6 V1.51 overlays to Normative — V1.51 Shipped`
10. `docs(indexes): V1.51 ship status in specs README, iterations index, deferred tracker`
11. `chore(status): Profile B compaction + V1.51 P-last residual resolution (8 resolved, 6 deferred to V1.52+)`
12. `style(nexus-orchestration): apply nightly rustfmt to match arm in auto_chronology_tick`
13. `test(nexus-daemon-runtime): update cron_supervisor_task for V1.51 run_one_tick signature (workspace_dir path)`
14. `test(nexus-orchestration): update novel-review-master version assertion to 3 (V1.51 T-A P0)`
15. `chore(status): V1.51 P-last ship metadata — PR #65 merged, PR #64 to main open; GitNexus index counts updated`

### WL-A fixes (8)

| ID | Scope | Fix |
|---|---|---|
| R-V151Q1-02 | `nexus-orchestration/src/tasks/mod.rs` | `doc_markdown` backticks |
| R-V151Q1-08 | `nexus-daemon-runtime/tests/cron_lock_integration.rs` | `TempDir::keep()` instead of deprecated `into_path()` |
| R-V151Q1-09 | `nexus-local-db/src/file_lock.rs` | Added rationale comment for parse-failure conflict path |
| cron-brainstorm-write S-002 | `nexus-orchestration/src/schedule/supervisor.rs` | Removed stale `COORDINATE-WITH-T-A-P2` marker |
| kb-editor-cli S-V150KBED-QC2-02 | `nexus42/src/commands/creator/world/kb.rs` | Use `load` verb for pre-check errors |
| cron-review-staggering qc2 | `nexus-orchestration/tests/cron_supervisor.rs` | Idempotency test label + backticks |
| auto-chronology qc3 S-3 | `nexus-orchestration/tests/auto_chronology_tick.rs` | `doc_markdown` + `match_wildcard_for_single_variants` |
| cron-foundation qc1 S5 | `nexus-local-db/tests/works_schedule_migration.rs` | `doc_markdown` backticks |

### Spec overlays promoted to Normative — V1.51 Shipped (6)

- `.mstar/knowledge/world-kb-runtime-architecture.md`
- `.mstar/knowledge/specs/entity-scope-model.md`
- `.mstar/knowledge/specs/novel-writing/quality-loop.md`
- `.mstar/knowledge/specs/cli-spec.md`
- `.mstar/knowledge/specs/concurrency.md`
- `.mstar/knowledge/specs/llm-extract.md`

### Index updates

- `.mstar/knowledge/specs/README.md`
- `.mstar/iterations/README.md`
- `.mstar/knowledge/deferred-features-cross-version-tracker.md`

### Profile B compaction

- Archived 7 V1.51 `Done` plan objects to `.mstar/archived/plans/<plan-id>.json`.
- Appended 7 string plan IDs to `.mstar/archived/plans-done.json` (Profile B invariant verified).
- Added `iteration_summaries["v1.51"]` to `.mstar/archived/plans-done.json`.

### Residual resolution

- Resolved 8 residuals in `status.json`.
- Deferred 6 non-V1.51 items to V1.52+ with `lifecycle: deferred`.
- `metadata.tech_debt_summary`: `total_open = 0`, `total_deferred = 6`, `total_resolved = 59`.

### Test drift fixed

- `nexus-daemon-runtime/tests/cron_supervisor_task.rs`: updated `run_one_tick` calls for new `workspace_dir: &Path` signature.
- `nexus-orchestration/src/preset/mod.rs`: updated `novel-review-master` version assertion from `2` to `3` (V1.51 T-A P0).

---

## Verification

| Command | Result |
|---|---|
| `cargo +nightly fmt --all --check` | Clean |
| `cargo clippy --all -- -D warnings` | Clean |
| `cargo test --all` | All passing (previously-failing `embedded_novel_review_master_loads_and_validates` fixed) |
| Profile B invariant | Verified: `plans-done.json` `plans` array contains only strings |

---

## Pull requests

- **#65** — `feature/v1.51-hygiene-and-closeout` → `iteration/v1.51` (P-last body; merged)
- **#66** — `feature/v1.51-hygiene-and-closeout` → `iteration/v1.51` (ship metadata follow-up; merged)
- **#64** — `iteration/v1.51` → `main` (final integration PR; open, awaiting merge)

---

## Notes

- Wire contracts unchanged (compass §0.1 #8).
- Platform integration remains paused; this iteration is local-only.
- `iteration/v1.51` should be retired after PR #64 merges.
- 6 residuals were intentionally deferred to V1.52+ rather than silently fixed; they are tracked in `status.json` with `lifecycle: deferred`.
