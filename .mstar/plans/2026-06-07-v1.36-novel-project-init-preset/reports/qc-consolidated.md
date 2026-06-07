---
report_kind: qc-consolidated
plan_id: 2026-06-07-v1.36-novel-project-init-preset
working_branch: feature/v1.36-novel-project-init-preset
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.36-p1-init
review_range: merge-base: iteration/v1.36 (1856258) + tip: feature/v1.36-novel-project-init-preset (2a97858)
generated_at: 2026-06-07T18:03:00+08:00
qc_seats: [qc-specialist, qc-specialist-2, qc-specialist-3]
wave: initial
verdict: Request Changes
---

# V1.36 P1 — QC Consolidated

## Reviewer verdicts (initial wave)

| Seat | Focus | Verdict | Critical | Warning | Suggestion | Report commit |
|------|-------|---------|---------:|--------:|-----------|---------------|
| qc-specialist | Architecture coherence | Request Changes | 2 | 1 | 1 | `e891397` |
| qc-specialist-2 | Security + correctness | Request Changes | 4 | 3 | 0 | `ed4ab71` |
| qc-specialist-3 | Performance + reliability | Request Changes | 0 | 4 | 2 | `ce008b4` |
| **Total** | | **Request Changes** | **6** | **8** | **3** | |

## Consolidated findings (deduplicated, sorted by severity then spec ref)

### Critical (must fix before Approve)

| ID | Source | Title | Spec / Plan ref |
|----|--------|-------|-----------------|
| C-001 | qc1 | `novel-project-init` preset never invokes `novel.project_scaffold` capability from its terminal state. The preset YAML declares the capability in `requires_capabilities` but the state graph has no `enter:` action calling it. | Plan §4.1 (T1); Spec §5.4 |
| C-002 / C-2 / W-3 | qc1+qc2+qc3 | **Scaffold atomicity broken** — FS ops (mkdir + template writes), `work_chapters` row inserts, and `works` PATCH are not in a single atomic transaction. Partial failure leaves orphaned dirs/files or duplicate rows. | Spec §5.4.3 "Atomicity: the entire scaffold (mkdir + template copies + work_chapters inserts + works PATCH) must succeed or fail together" |
| C-1 | qc2 | **Unsanitized `work_ref` path traversal** — grill-me-collected `work_ref` is used directly in path joins (`Works/<work_ref>/...`) without validation. An LLM-supplied or typo value with `..` or `/` can escape the workspace. | Spec §5.4.1 (paths); standard OWASP path-traversal |
| C-3 | qc2 | **No `world_id` FK existence check** before binding to a `works` row. A stale or attacker-supplied `world_id` binds to a non-existent world; downstream prompts reference nothing. | Spec §3.5 |
| C-4 | qc2 | **Untrusted ACP/grill-me responses used directly as FS paths and DB slugs** without sanitization. Same class as C-1 but generalized to `slug`, `genre`, `chapters` ranges, etc. | Plan §4.1 (T1 prompts) |

### Warning (must fix or document before Approve; targeted re-review)

| ID | Source | Title | Spec / Plan ref |
|----|--------|-------|-----------------|
| W-1 | qc3 | **Template engine divergence** — scaffold uses `String::replace` (custom) instead of `handlebars-rust` declared in `orchestration-engine.md §7.3` and spec §5.4.2. | Spec §5.4.2 ("Substitutes preset input vars (work_ref, title, world_id, etc.) using handlebars-rust per orchestration-engine.md §7.3") |
| W-2 | qc3 | **Unbounded `total_planned_chapters`** — `seed_chapters` accepts any `i32`; spec §5.4.3 says "1..N" but does not bound N. 1..100 should be the upper bound (matches `init-chapters.md` prompt). | Spec §5.4.3 |
| W-2-qc2 | qc2 | **`works` PATCH overwrites all novel columns on re-init** — broader than spec §5.4.4 "PATCH only updates fields the user explicitly changed in this grill-me session." | Spec §5.4.4 |
| W-001 | qc1 | **CLI `--init-preset` scheduling seam does not pass usable Work context** to the orchestrator. The `--init-preset` flag is parsed and stored, but the orchestrator's Start handler does not thread the `work_ref`/`total_planned_chapters`/`world_id` collected from grill-me back into the create-Work flow. | Plan §4.5 (T5); Spec §5.4.4 |
| W-1-qc2 | qc2 | **Concurrent re-init race** not mitigated or documented. Two simultaneous `novel-project-init` invocations on the same Work could each mkdir + insert, with the second winning on `works` PATCH but leaving orphan DB rows / FS files. (Pre-1.0 single-user, severity: Warning.) | Plan §4.6 (T6) |
| W-3 | qc2 | Pre-existing R-V133P1-09 (runtime `sqlx::query` vs compile-time `query_as!` for static DML on `works` table) — **not worsened by P1**, but the new `work_chapters.rs` also uses runtime `query` for some INSERTs. Note and document. | `nexus-local-db/AGENTS.md` |
| W-4 | qc3 | **Logging gap** — no "scaffold started" / "scaffold complete" structured logs. No warning when `pool` is `None`. | Spec §5.4.3 (atomicity observability) |

### Suggestion (defer / backlog)

| ID | Source | Title |
|----|--------|-------|
| S-001 | qc1 | Convert runtime `work_chapters` SQL to compile-time checked `query_as!` after schema stabilization (follow-up to R-V133P1-09). |
| S-1 | qc3 | Template cache consideration for high-volume use (out of V1.36 scope; single-workload). |
| S-2 | qc3 | Future CLI arg upper bound for `total_planned_chapters` (e.g. clap `value_parser`). |

## Consolidated verdict

**Request Changes** — initial wave. PM dispatches a fix wave to `@fullstack-dev-2` addressing the 5 Criticals + 7 Warnings (S-001 and Suggestion items deferred to backlog). After the fix wave, **targeted re-review** by all 3 QC seats (all 3 raised blocking findings, so N=3). Re-review scope: same `Review cwd` + `Working branch` + `plan_id` + `Review range` (post fix-wave `tip`).

## Re-review alignment fields (targeted)

When PM dispatches the re-review wave, the Assignment's 4 alignment fields must be text-identical to this consolidated header (only `tip:` may change to the post-fix commit).

## PM dispatch plan

1. **Fix wave** to `@fullstack-dev-2` (single message, single task): address 5 Criticals + 7 Warnings in surgical commits.
2. **Targeted re-review** to all 3 QC seats (one message, 3 tasks) with the 4 alignment fields above.
3. **Consolidate re-review** — if all 3 Approve, dispatch `@qa-engineer` for verification.
4. **QA verify** then merge `feature/v1.36-novel-project-init-preset` → `iteration/v1.36` and mark P1 Done in `status.json`.
5. **P2 dispatch** to `@fullstack-dev-2` immediately after P1 close.
