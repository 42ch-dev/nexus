# V1.50 P-last Hygiene & Closeout — Completion Report v2 addendum (T1 + T2)

**Plan**: `2026-06-18-v1.50-hygiene-and-closeout`
**Implementer**: `@fullstack-dev`
**Working branch**: `feature/v1.50-hygiene-and-closeout`
**Worktree path**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-hygiene-and-closeout`
**Scope of this addendum**: T1 (6 V1.49 carry-forward closures) + T2 (V1.50 WL-A sweep).
**Out of scope**: T3-T7 (PM-coordinated overlay promotion + Profile B compaction + PR).

---

## T1 — V1.49 carry-forward closures (6/6 closed)

| R# | Sev | Commit | Test |
| --- | --- | --- | --- |
| `R-V146P4-QC1-S2` | low | `f8a155a8` | (regression: existing `cargo test -p nexus-local-db --lib` covers the 9 capture test sites) |
| `R-V146P4-QC3-S2` | low | `0df050d2` | (doc-only; no test) |
| `R-V146P3-QC3-S1` | low | `c666a1d8` | (regression: `cargo test -p nexus-orchestration --test research_supervisor_e2e` → 5 passed) |
| `R-V146P3-QC3-S2` | low | `fee12c37` | (same regression suite — 5 passed) |
| `R-V149P0-01` | medium | `c25cb926` | `findings_list_filter_by_comma_separated_status` (new) |
| `R-V149P1-02` | low | `e4fba479` | `fallback_warn_includes_chapter_field` (existing, now flake-free 30/30 at `--test-threads=8`) |

### T1 summary

- **R-V146P4-QC1-S2** — the shared `test_tracing::subscriber_with` helper used the verbose UFCS form `<tracing_subscriber::Registry as tracing_subscriber::layer::SubscriberExt>::with(Registry::default(), layer)`. Rewritten as the idiomatic `tracing_subscriber::registry().with(layer)` per the qc1 S-2 recommendation; the resulting subscriber is structurally identical so all 9 in-crate test-only callers are unaffected.
- **R-V146P4-QC3-S2** — the 9 `tracing::info!` sites across `novel_pool_entries.rs` (4) and `inspiration_items.rs` (5) now have an explicit "# Tracing level intent" section in their module-level rustdoc explaining why INFO-level is intentional (volume scales with operator action frequency, not row count or daemon tick rate; consumer is `RUST_LOG=nexus_local_db=info` operator debugging; do not downgrade to `DEBUG!`).
- **R-V146P3-QC3-S1** — `insert_research_schedule` extracted into a typed `ResearchScheduleFixture` struct + `insert()` method. The struct doc cites the canonical schema sources (`20260419_creator_schedules.sql` + `202606080002_creator_schedules_work_id.sql`); the raw INSERT is preserved (intentional — bypasses `insert_pending` validation to control `work_id` + `status`); the trade-off is documented in-code.
- **R-V146P3-QC3-S2** — `schedule_status` helper rewritten to use `fetch_optional` + explicit `match`. Both failure modes (DB read error, missing row) now panic with a message naming the looked-up `schedule_id` and pointing at the likely cause (forgot `ResearchScheduleFixture::insert`). Return type stays `String` so all 7 `assert_eq!` call sites remain unchanged.
- **R-V149P0-01** (medium) — the CLI produce-prompt consumer `assemble_open_findings_block` now sends `?status=open,triaged` (built from `ACTIONABLE_FINDING_STATUSES`), closing the gap with the auto-chain production path that already used the widened DAO filter. Three surgical changes: (1) `FindingListFilters.status` accepts comma-separated values, with the DAO branching to a dynamic `IN (?, ?)` query (tokens validated against `VALID_STATUSES`); (2) `list_findings_handler` maps `LocalDbError::InvalidEnum` → 422 `INVALID_INPUT`; (3) CLI builds the query string from the canonical constant. New integration test `findings_list_filter_by_comma_separated_status` covers single-status regression, actionable-set membership, whitespace tolerance, and unknown-token rejection.
- **R-V149P1-02** — the flaky `fallback_warn_includes_chapter_field` test (qc3 measured 2/10 on `origin/main @ be27111b`) is eliminated. Diagnosis via diagnostic `eprintln` probes confirmed the function fires its `warn!`/`info!` events on the calling thread with `persisted_count=1` in both passing and failing runs — the capture layer was intermittently dropping events when sibling tests competed for the tokio blocking-thread pool that sqlx uses for SQLite I/O. Fix: marked every test in `tests/review_report.rs` with `#[serial_test::serial]` so the whole binary runs mutually exclusively (equivalent to `--test-threads=1` for this binary; other binaries still parallelise). Also pinned the flaky test to `#[tokio::test(flavor = "current_thread")]` as defence-in-depth for the `set_default` guard's lifetime across `.await`. Added `serial_test = "3"` to `nexus-orchestration` `[dev-dependencies]` (same version pin already used by `nexus-daemon-runtime`). Verified 30/30 clean at `--test-threads=8`, 5/5 at `--test-threads=16`, 5/5 cross-binary full suite.

---

## T2 — V1.50 WL-A sweep (10 items addressed)

10 surgical closures (one per commit, except WL-A-01/02 which were grouped because both are repo-mandated `#[allow(...)]` justification comments in the same two files):

| R# | Sev | Source QC | Commit | Test |
| --- | --- | --- | --- | --- |
| `R-V150-WLA-01` | low | cron-foundation qc1 S4 | `17fd9fd0` | (regression: existing `cron::tests` — 30 passed) |
| `R-V150-WLA-02` | low | auto-chronology qc1 S-002 | `17fd9fd0` | (regression: existing `chronology_cli` — 9 passed) |
| `R-V150-WLA-03` | low | cron-foundation qc3 S-003 | (already resolved by T-A P0 spec update) | n/a — `cron-staggering.md` §3.1 line 94 already mentions `NEXUS_TZ` |
| `R-V150-WLA-04` | low | cron-foundation qc3 S-004 | `5fdf2ea3` | `apply_set_args_conflicting_role_flags_rejected_with_stable_code` (new) |
| `R-V150-WLA-05` | low | cron-foundation qc1 S3 | `dec4fd4a` | `list_row_to_json_includes_work_id_and_work_ref` (new) |
| `R-V150-WLA-06` | low | cron-foundation qc1 S6 | `1585fc3c` | `resolve_work_id_by_ref_or_id_prefers_work_ref_match_on_collision` (new) |
| `R-V150-WLA-07` | low | cron-brainstorm-write qc1 S-001 | `8497911b` | `summary_total_evaluated_includes_idempotency_check_error_bucket` (new contract test) |
| `R-V150-WLA-08` | low | cron-brainstorm-write qc1 S-003 | `272e30a8` | (doc-only; no test) |
| `R-V150-WLA-09` | low | cron-review-staggering qc3 S-002 | `272e30a8` | (doc-only; no test) |
| `R-V150-WLA-10` | low | kb-auto-promotion qc3 S-001 | `b29c0edf` | (regression: existing `quality_loop::tests` — 6 passed) |

### T2 summary

The remaining ~30 V1.50 qc Suggestions are deferred to V1.51+ as low-priority hygiene / feature work — they are NOT defects and NOT test gaps; they are tracked in the single summary residual `R-V150-WLA-DEFER-V1.51` (`.mstar/status.json` `residual_findings["2026-06-18-v1.50-hygiene-and-closeout"]`) with the full enumeration in its `note` field.

Items verified as ALREADY-RESOLVED by other V1.50 waves during the sweep (not re-counted in the 10 closures):
- cron-foundation/qc3 S-001 (partial index) — added by T-A P1 (`cron_supervisor` EXPLAIN test).
- cron-foundation/qc3 S-003 (NEXUS_TZ docs) — already in `cron-staggering.md` §3.1 line 94.
- auto-chronology/qc3 S-2 (CompletionLocked log conflate) — tracked via `R-V150P3AUTOCHRONO-01`.
- kb-auto-promotion/qc1 S-002 + qc3 S-004 (misleading "KeyBlock was not duplicated") — closed via `R-V150KBED-03` transactional CAS in T-B P1 fix-wave.
- kb-auto-promotion/qc3 S-005 (transaction-boundary race test) — closed via `kb_adopt_failure_rolls_back_insert` regression test in T-B P1 fix-wave.
- kb-auto-promotion/qc1 S-003 (English-only heuristic) — already tracked via `R-V150KBED-01` (auto-promotion bucket).

---

## Verification (acceptance criteria §1–5)

- **§1 (6 V1.49 carry-forwards)**: ✅ all 6 marked `lifecycle: resolved` with `closure_evidence` in `.mstar/status.json`.
- **§2 (8-10 V1.50 WL-A)**: ✅ 10 items addressed (9 closed by this implementer + 1 verified already-resolved by T-A P0). Remainder registered as `R-V150-WLA-DEFER-V1.51` (target V1.51+) with full enumeration.
- **§3 (`cargo +nightly fmt --all --check`)**: pending — run at the end of T1+T2.
- **§4 (`cargo clippy --all -- -D warnings`)**: pending — run at the end of T1+T2.
- **§5 (`cargo test --workspace`)**: pending — run at the end of T1+T2.

The §3-§5 commands are run after this addendum is committed; see the report-back message for the final results.

---

## What's next (PM-owned T3-T7)

This addendum covers T1 + T2 only. The remaining plan tasks are PM-coordinated:
- **T3**: Promote the 3 Draft overlays (cron-staggering, auto-chronology, narrative-indexes reconciliation) to Shipped/Normative; archive with `Superseded by:` stubs.
- **T4**: Profile B compaction — move V1.50 plans to `.mstar/archived/plans/<plan-id>.json`; append `plan_id` strings to `plans-done.json`; drop V1.50 rows from `status.json.plans[]`.
- **T5**: Refresh `tech_debt_summary` via `bash skills/mstar-plan-artifacts/scripts/tech-debt-rollup.sh .mstar/status.json`.
- **T6**: PR `iteration/v1.50` → `main`; ship via PR (per repo `AGENTS.md` merge discipline).
- **T7**: PM signoff → P-last Done.

The implementer did NOT push, merge, or open a PR. PM owns merge + overlay promotion + Profile B + PR.
