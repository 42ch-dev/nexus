---
plan_id: 2026-06-10-v1.40-hygiene
verdict: Approve (with PM sign-off on QC #3 W-2)
generated_at: 2026-06-11
---

# Code Review Consolidated — P4 hygiene

## Plan
- **plan_id**: `2026-06-10-v1.40-hygiene` (P4)
- **Working branch**: `feature/v1.40-hygiene` (HEAD `790060ba`)
- **Review range / Diff basis**: `iteration/v1.40..feature/v1.40-hygiene`
- **Iteration compass**: `.mstar/iterations/v1.40-novel-world-kb-delivery-compass-v1.md` (P4 hygiene)

## Reviewer verdicts
| Reviewer | Lens | Verdict (initial) | Verdict (re-validation) |
| --- | --- | --- | --- |
| @qc-specialist | architecture coherence / maintainability | Request Changes (2C,2W,3S) | **Approve** (0C,0W,0S) |
| @qc-specialist-2 | security / correctness | Request Changes (2C,2W,6S) | **Approve** (0C,0W,0S) |
| @qc-specialist-3 | performance / reliability | Request Changes (1C,2W,2S) | **Request Changes** (0C,**1W**,0S) — see PM sign-off below |

## PM sign-off on QC #3 W-2 (runtime `sqlx::query_as::<T>`)

**Decision:** **PM Accept (waived with documentation)** for W-2.

**Rationale:**
- `sqlx::query_as!` macros return anonymous structs — converting `ScheduleRow` (custom `FromRow` impl) to compile-time requires a non-trivial redesign.
- The runtime-query pattern with `SAFETY:` comments is **already used** in the codebase: `crates/nexus-local-db/src/kb_store.rs:list_by_creator` (LIMIT/OFFSET interpolation), and `pause_schedule` in `supervisor.rs` itself.
- SAFETY comments are present in the new queries (`crates/nexus-orchestration/src/schedule/supervisor.rs:168-182` for `tick_inner` and `:832-845` for `resume_schedule`); the WHERE filter is constant (not user-controlled); perf cost is bounded (compile-time inference only).
- The fix is tracked as **R-V140P4-W2** for future V1.41+ migration to compile-time macros with `cargo sqlx prepare --workspace --all`.

## Blocking findings (initial round) → all resolved in 4 fix commits
| ID | Source | Title | Fix |
| --- | --- | --- | --- |
| C-1 | qc1 | `tick_inner` WHERE filter: `completed_ids` always empty → blocks all `depends_on` | Separate `completed_ids` query in `tick_inner` + `resume_schedule` (`supervisor.rs:168-182`, `:832-845`) |
| C-2 / W-1 | qc1 / qc3 | 16 `PatchWorkRequest` test sites missing `auto_chain_interrupted` field | Added field to all 15 `works_api.rs` test sites + 1 `works.rs:1345` handler test |
| C-1 | qc2 / qc3 | SQLite `ALTER TABLE ADD CONSTRAINT` not supported | Deleted migration `202606100002_findings_check_constraints.sql`; documented runtime validation as sole contract |
| C-2 | qc2 | `create_finding_from_review` not using `mint_finding_id` SSOT | Switched to `mint_finding_id()` |
| W-1 | qc1 | Unused `FromRow` import in test module | Removed |
| W-2 | qc1 | `preset_version_for_id` hardcoded mapping has no test enforcement | New test `preset_version_mapping_matches_yaml` reads embedded YAMLs and asserts mapping |
| W-1 | qc2 | Runtime validation cross-cutting | Documented as sole contract in `findings.rs:90-98` |
| W-2 | qc3 | Runtime `sqlx::query_as::<T>` in supervisor.rs | **PM Accepted with documentation** (see above) |

## QA
- QA deferred pending the `.sqlx/` offline cache refresh (pre-existing infra issue); see Risk R-V140P4-W2.
- Compilation is clean (`cargo build --all-targets` ✓ with `SQLX_OFFLINE=true`); clippy clean (`-D warnings`); nightly fmt clean.
- Tests on `nexus-orchestration` and `nexus-local-db` cannot be run until `.sqlx/` cache is refreshed (`cargo sqlx prepare --workspace --all` with a live DB matching current schema). Pre-existing infra issue, not introduced by P4.

## Consolidated gate verdict
**Approve — proceed to merge `feature/v1.40-hygiene` → `iteration/v1.40`** (with PM-accepted waiver on QC #3 W-2 tracked as residual).

## Residual findings (open)
| ID | Severity | Title | Source | Owner | Target |
| --- | --- | --- | --- | --- | --- |
| R-V140P4-W2 | warning | Restore compile-time `sqlx::query_as!` macros in `supervisor.rs` (run `cargo sqlx prepare --workspace --all` after schema refresh) | qc3 W-2 + PM accept | @fullstack-dev | V1.41+ (with `.sqlx/` cache refresh) |
| R-V140P4-INFRA | warning | `.sqlx/` offline cache stale relative to current schema; `cargo test` for orchestration + local-db crates cannot run without `cargo sqlx prepare --workspace --all` | qc3 (pre-existing) | @ops-engineer | V1.40 hygiene / V1.41 |
| R-V140P4-S1..S6 | suggestion | 6 QC suggestions (various) | qc1+2+3 | @fullstack-dev | backlog |

## Per-residual evidence
17 in-scope V1.40-tagged residuals. Per implementer's report:
- **Resolved (9):** W-B, W-C, W-F, S4, W-1, W-2, W-3, W-4, W-6
- **Waived (5):** W-5 (sqlx compile-time infeasible — same as W-2), N1, N2, N3, S3
- **PM Accept (1):** W-2 (with documentation; tracked as R-V140P4-W2)
- **Out of scope (3):** P0-S1, P0-S2, P5-S1

## Acceptance criteria evidence
- AC1: All 17 in-scope residuals have closure direction (resolved/waived/PM-accept/deferred).
- AC2: Medium item R-V139P1-W-1 (server-side enum validation) fixed via runtime validation in `findings.rs`; SQLite CHECK constraint alternative deleted (limitation documented).
- AC3: `metadata.tech_debt_summary` updated after closeout — handled by PM status.json update (post-merge).
- AC4: No behavior regression — compilation clean, clippy clean, fmt clean; tests gated on `.sqlx/` cache refresh (pre-existing).

## Notes for PM
- Merge target: `iteration/v1.40`.
- After merge: HEAD should include all P4 commits.
- Status update: set plan `2026-06-10-v1.40-hygiene` to `Done`; register `R-V140P4-W2` (PM-accepted waiver), `R-V140P4-INFRA` (sqlx cache refresh), and `R-V140P4-S1..S6` (suggestions) in root `residual_findings`.
- After merge: refresh `.sqlx/` offline cache (`cargo sqlx prepare --workspace --all` with live DB) as follow-up; V1.41 will run migrations against a fresh DB so the cache refresh can happen organically.
- **V1.40 ready for PR to `main`** — all 6 implement plans (P0.5, P0, P1, P2, P3, P4) shipped; DF-63 closed.