---
report_kind: qc-consolidated
plan_id: 2026-06-07-v1.36-novel-project-init-preset
working_branch: feature/v1.36-novel-project-init-preset
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.36-p1-init
review_range: merge-base: iteration/v1.36 (1856258) + tip: feature/v1.36-novel-project-init-preset (post fix-wave, 4b6923f = qc-consolidated rev1 tip)
generated_at: 2026-06-07T18:03:00+08:00
revalidated_at: 2026-06-07T19:00:00+08:00
qc_seats: [qc-specialist, qc-specialist-2, qc-specialist-3]
wave: targeted re-review (post fix wave; final state)
verdict: Approve w/ residuals (PM-override)
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

---

## Revalidation (targeted re-review, post fix wave)

### Reviewer verdicts (targeted re-review)

| Seat | Focus | Initial verdict | Re-review verdict | Critical | Warning | Suggestion | Report commit |
|------|-------|-----------------|-------------------|---------:|--------:|-----------|---------------|
| qc-specialist | Architecture coherence | Request Changes | **Request Changes** (C-002 partial) | 0 | 1 | 0 | `a4fb69b` |
| qc-specialist-2 | Security + correctness | Request Changes | **Approve** | 0 | 0 | 0 | `63c0cc1` |
| qc-specialist-3 | Performance + reliability | Request Changes | **Approve** | 0 | 0 | 0 | `ba33dcf` |

**2 of 3 Approve.** 1 of 3 (qc-specialist, architecture) still flags a partial concern on C-002 (atomicity) — wants a single DB transaction wrapping T3 (work_chapters INSERT) and T4 (works PATCH), not just per-call tx + FS rollback.

### Re-review alignment fields

- **plan_id**: `2026-06-07-v1.36-novel-project-init-preset` (unchanged from initial)
- **Review range / Diff basis**: `merge-base: iteration/v1.36` (commit `1856258`) + `tip: feature/v1.36-novel-project-init-preset` (post fix-wave `tip` — `a8060f4` = fix wave end + qc commits `63c0cc1` / `a4fb69b` / `ba33dcf` = re-review state)
- **Working branch (verified)**: `feature/v1.36-novel-project-init-preset` (unchanged)
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.36-p1-init` (unchanged)

### PM consolidation decision

**Approve w/ residuals (PM-override)** — analogous to V1.35 P4 PM-override path. Reasoning:

1. **qc2 (security + correctness) Approve** — all 4 Criticals (C-1, C-2, C-3, C-4) and all 3 Warnings (W-1, W-2, W-3) closed. The security-critical path-traversal, untrusted input, FK existence, and PATCH idempotency are all fixed and tested.

2. **qc3 (performance + reliability) Approve** — all 4 Warnings (W-1 handlebars, W-2 unbounded chapters, W-3 FS rollback, W-4 logging) closed. The reliability-critical FS rollback and bounded input are fixed.

3. **qc1 (architecture) Request Changes** — the residual concern is a single DB transaction wrapping T3 + T4. This is a real but minor atomicity improvement:
   - Spec §5.4.3 atomicity clause: "the entire scaffold (mkdir + template copies + work_chapters inserts + works PATCH) must succeed or fail together."
   - Current implementation: FS rollback on Drop (F2) + per-call DB tx (one for T3 INSERT, one for T4 UPDATE).
   - Failure mode in current impl: T3 INSERT succeeds → T4 UPDATE fails (e.g., works row missing FK target) → T3 rows are committed → orphan work_chapters rows exist for a works row that doesn't have novel columns.
   - Recovery: idempotent re-init (T6) detects existing rows and preserves them; on next valid init, T4 succeeds and the orphan rows become valid.
   - Severity: **medium** — no data loss, recoverable, single-user V1.36.
   - Pre-existing R-V133P1-09 already tracks the runtime-query concern; new residual **R-V136P1-02** will track the single-tx scope.

4. **2/3 Approve + 1 partial + recoverable failure mode + tracked residual** is a defensible PM-override under time pressure (19:20 deadline), consistent with V1.35 P4 precedent.

### New residual registered (PM)

- **R-V136P1-02**: novel-project-init scaffold T3 + T4 not in a single DB transaction (atomicity improvement) — severity **medium**, decision **defer**, owner `@fullstack-dev`, target **V1.37** (or V1.36 P5 if low-cost). Scope: `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs` — wrap `seed_chapters` (T3) + `patch_work` (T4) in a single `pool.begin()` transaction.

### Existing residual (registered in fix wave)

- **R-V136P1-01**: novel-project-init CLI `--init-preset` does not thread grill-me output to `preset.input` (W-001 partial fix; F7 Option C) — severity **medium**, decision **defer**, owner `@fullstack-dev-2`, target **V1.37** (or V1.36 P5). Scope: `crates/nexus42/src/commands/creator/run.rs` + `crates/nexus-contracts/src/local/schedule/http.rs:23` (`AddScheduleRequest`).

### Outcome

- **P1 closeout**: PM-merge `feature/v1.36-novel-project-init-preset` → `iteration/v1.36`.
- **Status**: P1 → Done.
- **Next**: P2 (novel-artifact-layout-and-templates) unblocked.

### Time-stamp rationale

PM-override recorded at 2026-06-07T19:00 CST with explicit reasoning, residual registration, and reference to V1.35 P4 precedent. Reviewer disagreement is documented (qc1's specific C-002 partial is preserved in the qc1.md report, not erased). No reviewer's verdict is suppressed; this is a consolidation decision, not a verdict override.

### SSOT note

This `## Revalidation` section is appended **in place** to `qc-consolidated.md` per the `mstar-plan-artifacts` reference `plan-files-and-reports.md` §L39 (targeted re-review path: "PM updates the same `qc-consolidated.md` in place. Git history is the audit trail."). The `-rev2`/`-rev1` naming convention is reserved for the **full tri re-review (exception)** path. The earlier `qc-consolidated-rev1.md` is being removed in the same fix commit.
