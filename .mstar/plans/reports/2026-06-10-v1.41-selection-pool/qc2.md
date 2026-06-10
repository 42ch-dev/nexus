---
report_kind: qc-review
reviewer: "@qc-specialist-2"
reviewer_index: 2
focus: security-correctness
plan_id: 2026-06-10-v1.41-selection-pool
verdict: Request Changes
generated_at: 2026-06-10T22:18:00+08:00
review_range: "merge-base: 55689706 → tip: 57f573ad"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
files_reviewed: 12
tools_run: cargo clippy (4 crates), cargo +nightly fmt --all -- --check, cargo test (4 crates + 9 new hermetic), git log/diff/stat, manual source review
---

# Code Review Report — V1.41 P1 (qc2)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-10T22:18:00+08:00

## Scope
- plan_id: 2026-06-10-v1.41-selection-pool
- Review range / Diff basis: merge-base: 55689706 → tip: 57f573ad
- Working branch (verified): iteration/v1.41
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 12 (focus on 7 P1 files + supporting migrations, auto_chain, handlers P1 section, CLI works, selection_pool tests)
- Commit range: 55689706..57f573ad (7 P1 commits under focus: b3a1f023, dfff13f8, 8066caf6, 78c89aad + supporting)
- Tools run: cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings; cargo +nightly fmt --all -- --check; cargo test on the 4 crates; git log/diff/stat; manual source + checklist review per mstar-review-qc + assignment security/correctness items

## Findings

### 🔴 Critical
- (none)

### 🟡 Warning
- **W-01 (authz / creator scoping on mutate-by-id paths)**: `promote_inspiration_handler` (handlers/works.rs:1496) fetches the inspiration item by `item_id` only (`get_inspiration` has no creator filter), then creates the Work + pool row under the *current session's* `creator_id` (from `read_active_creator_id`), and calls `promote_inspiration(item_id, ...)` which does a bare `UPDATE ... WHERE item_id = ?`. No verification that `item.creator_id == creator_id`. The resulting Work and pool entry are attributed to the acting creator while the source inspiration row may have belonged to another. Same pattern in `archive_inspiration_handler` (bare `item_id` UPDATE) and `archive_pool_entry_handler` (bare `entry_id` UPDATE). Pool `promote` path correctly does `works::get_work(..., &creator_id, &req.work_id)` before acting; the new P1 inspiration/archive paths do not. This violates the explicit "creator_id checks" requirement in the assignment for `creator works pool *` and `inspiration *` subcommands. (Source: handlers/works.rs:1496–1585, 1594–1618, 1385–1407; inspiration_items.rs:241 (get), 190 (promote), 219 (archive); novel_pool_entries.rs:159 (archive); contrast with 1355 (pool promote).)

- **W-02 (incomplete atomicity on inspiration promote)**: On success path the handler performs three separate DB operations after the initial item lookup: (1) `works::create_work`, (2) `promote_to_active` (which is itself a tx for pool), (3) `promote_inspiration` status update. If (3) fails after (1)+(2) succeed, a Work + active pool row exist with no `promoted_work_id` on the inspiration item (orphan). No compensating transaction or cleanup. (Source: handlers/works.rs:1558–1585; auto_chain.rs:256 for the related mark_work_completed pattern.)

- **W-03 (best-effort pool row update on completion)**: `mark_work_completed` (auto_chain.rs:256) does the Work patch (status + completion_locked_at + novel_completion_status), then best-effort `mark_pool_entry_completed_for_work` (warn + continue on error), then logs that the supervisor must write the lockfile. The pool update has a guard (`status != 'completed'`) but is non-atomic with the Work patch and non-fatal. If it fails, the pool row can remain `active`/`queued` while the Work is `completed` + locked. The assignment explicitly asks about the "3 writes" recovery story; the code documents the non-fatal nature but leaves a divergence window. (Source: auto_chain.rs:279–292, novel_pool_entries.rs:209 (the UPDATE), 218 (the WHERE guard).)

- **W-04 (unbounded list for pool + inspiration)**: `list_pool` and `list_inspiration` (and their CLI renderers) have no `LIMIT` / `OFFSET` or hard cap in DAO or handler. `list_pool_entries` / `list_inspiration` in the DAOs do `ORDER BY ...` with optional status filter and return everything for the creator. At 1000+ entries this will return the full set (potential memory/UX issue on very large local histories). The assignment calls this out explicitly. Current local-desktop usage makes it low severity, but it is a latent correctness/scalability gap for the list paths. (Source: novel_pool_entries.rs:57, inspiration_items.rs:57; handlers/works.rs:1313, 1452; CLI works/mod.rs:501, 641.)

### 🟢 Suggestion
- **S-01 (slug UX for non-ASCII titles)**: `title_to_slug` (inspiration_items.rs:263) maps any non-ASCII to `-`, collapses, and for pure CJK input returns "untitled" (tests confirm this). The file existence check + DB unique index on `(creator_id, rel_path)` prevents collision, but users typing Chinese/Japanese titles will always get the fallback name. Consider a short-id fallback (e.g., `inspiration-<8hex>`) or preserving a sanitized prefix when the ASCII projection collapses to empty. Safe today, but poor UX for the primary persona.

- **S-02 (archive / promote handlers should take creator_id explicitly)**: Even though the local threat model is single-user, the DAO layer and archive/promote-by-id handlers should accept (and assert) the acting `creator_id` and add `AND creator_id = ?` to the UPDATEs (or do a get-then-verify before mutate). This matches the pattern already used for Work operations and the pool promote path. Adds defense-in-depth and makes the "creator_id checks" explicit in the new resource paths.

- **S-03 (set-default on pool promote is not atomic at CLI layer)**: CLI `handle_pool_promote` posts to `/pool/promote`, then (if `--set-default`) posts a second time to the legacy `/pool` set_pool_active endpoint. Two round-trips; the second can fail independently. The request DTO still carries `set_default` (for compat) but the daemon handler ignores it. Consider either (a) making the daemon handler honor `set_default` inside the same tx, or (b) documenting that `--set-default` is best-effort.

- **S-04 (document the pre-existing unrelated test)**: The assignment notes `db::pool::tests::pool_config_from_env_reads_valid_values` (8 == 4). It did not appear in the test runs for the four crates in this review (all 47 + 15 + doc-tests passed cleanly). If it is a pre-existing flake outside the diff, record it as known and unrelated; no action required for this plan.

## Source Trace
- Finding ID: W-01
- Source Type: manual-reasoning + code review
- Source Reference: crates/nexus-daemon-runtime/src/api/handlers/works.rs:1496 (get_inspiration without creator), 1558 (create_work as current creator), 1567 (promote_to_active as current), 1575 (promote_inspiration by item_id only); contrast 1355 (pool promote does pass creator to get_work); inspiration_items.rs:241,190,219; novel_pool_entries.rs:159
- Confidence: High

- Finding ID: W-02
- Source Type: manual-reasoning
- Source Reference: handlers/works.rs:1558–1585 (three sequential calls after lookup)
- Confidence: High

- Finding ID: W-03
- Source Type: manual-reasoning + grep
- Source Reference: auto_chain.rs:279 (`if let Err(e) = novel_pool_entries::mark_pool_entry_completed_for_work... { tracing::warn! ... }`); 282–292; novel_pool_entries.rs:209 (the guarded UPDATE)
- Confidence: High

- Finding ID: W-04
- Source Type: manual-reasoning + source
- Source Reference: novel_pool_entries.rs:57 (`list_pool_entries` no limit), inspiration_items.rs:57 (same), handlers/works.rs:1320 and 1459 (pass-through), CLI list renderers
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

## Static Analysis & Verification
- `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings`: clean (finished dev profile, no diagnostics emitted).
- `cargo +nightly fmt --all -- --check`: clean (no output).
- `cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db`: all passed (47 unit/integration + 15 regression + 1+2+4+2 doc-tests across the crates; the 9 new hermetic selection_pool tests (TC1–TC9) all green).
- Git scope verified at session start per assignment:
  - `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`
  - `git branch --show-current` → `iteration/v1.41`
  - `git rev-parse --verify 57f573ad...` and `55689706...` both succeed and match assignment.
- Focus reads covered the 7 mandated files plus the P1 handler section, full CLI works module, auto_chain T7 hook, the two new DAOs, the inspiration migration, and the 448-line test suite.

## Checklist (mstar-review-qc + assignment)
- [x] Cwd/branch/SHAs verified before any review steps.
- [x] All mandatory static checks executed and clean.
- [x] Security: authorization (creator_id), path traversal (slug), SQL (parameterised with SAFETY comments), cross-creator data, set_pool_active tx, HTTP endpoints, FS writes — all evaluated.
- [x] Correctness: mark_work_completed + pool update, partial unique index, inspiration promote atomicity/rollback, archive cascade/FK, list pagination, --set-default, slug collision error, 2-step ceremony recovery — all evaluated.
- [x] No modifications to implementation or status.json (QC write scope respected).
- [x] Report uses exact template + frontmatter per mstar-review-qc and qc-specialist-shared.
- [x] Will commit **only** the report path, then emit real `git log -1` in Completion Report.

**Self-check before sign-off (per assignment)**: All 6 items answered YES after the above steps. Report will be committed and the turn will end with the Completion Report v2 (no follow-up questions).
