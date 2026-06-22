---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-22-v1.59-df47-manuscript-and-misc-capabilities"
verdict: "Request Changes"
generated_at: "2026-06-22"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-22

## Scope
- plan_id: 2026-06-22-v1.59-df47-manuscript-and-misc-capabilities
- Review range / Diff basis: merge-base: 578be5231f32463cd86ef38bd6af15c7f4bcc3f3 + tip: 95d3595c19b18656f393940c96750b642dc09ee1
- Working branch (verified): iteration/v1.59
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: ~15 (primary: crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs, crates/nexus-daemon-runtime/src/capability_registry.rs, crates/nexus-daemon-runtime/tests/cross_caller_e2e.rs, crates/nexus-daemon-runtime/src/db/schema.rs; plus .mstar/knowledge/specs/*.md deltas)
- Commit range: 578be523...95d3595c
- Tools run: git diff, git rev-parse, read (source + tests + specs), grep (NexusApiError, canonicalize, path patterns, sqlx query patterns, phase/works helpers)

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **W-001 (T3 — manuscript.write atomicity)**: `execute_manuscript_write` performs temp-file write + `tokio::fs::rename`, then a **separate** `sqlx::query("UPDATE work_chapters SET actual_word_count...")` **outside any transaction**. On crash between rename and UPDATE, the file is durable but `actual_word_count` is stale (data/metadata inconsistency). Compare to `execute_manuscript_chapter_update` (C-002 pattern) which wraps DB UPDATE + rename inside a single `BEGIN IMMEDIATE` tx. Assignment explicitly asks "is the atomic write correct?" — it is not.  
  **Source**: host_tool_handlers.rs:2147-2183 (rename at 2155, UPDATE at 2169-2183, comment claims "in the same transaction" but code does not).  
  **Fix**: either (a) adopt the tx+rename-inside-tx pattern from chapter_update, or (b) document+accept the window and add compensating reconciliation on read.

- **W-002 (T3 — missing defense-in-depth path guard)**: `manuscript.write` (and `read_range`) resolve `abs_body = workspace_root.join(body_path_from_db)` and perform FS ops **without** `canonicalize()` + `starts_with(workspace_root)` check before write/read. While `body_path` originates from trusted seed code (`work_chapters` at chapter creation), there is no runtime guard if that column is ever populated from untrusted input in the future. Older fs/* handlers (lines ~720-757) do perform this check.  
  **Source**: host_tool_handlers.rs:2135 (write), 2003 (read_range), 2122 (body_path extraction). No canonicalize in the 9 new handlers.  
  **Risk**: path traversal if DB trust is ever violated (defense-in-depth gap, not active exploit today).

- **W-003 (T6 — test nondeterminism)**: `workspace_paths_rejects_without_workspace` test accepts either success (if fixture seeds the path) or `INVALID_INPUT`. The assertion is intentionally weak ("accept either path"). This reduces regression protection for the "no workspace" failure mode that the handler claims to enforce.  
  **Source**: host_tool_handlers.rs tests (around workspace_paths_rejects_without_workspace).

### 🟢 Suggestion
- **S-001**: Extract a shared helper `write_chapter_body_atomically(pool, work_id, chapter, volume, content, workspace_root, body_path)` that encapsulates temp+tx+rename+word_count update + canonicalize guard. Both `manuscript.write` and `manuscript.chapter.update` can use it; reduces duplication and ensures the atomic pattern is applied uniformly.
- **S-002**: Add a bounded `canonicalize` + prefix assertion (even as a debug-only or tracing span) inside the manuscript write/read paths so that any future `body_path` tampering is immediately observable in logs/audit.
- **S-003 (T7)**: `research.query` list path does client-side tag filtering after fetching up to 1000 rows. For large tables this is fine (limit clamped), but consider pushing a `LIKE` or JSON tag filter into the SQL if tag queries become hot. Current implementation has no SQL injection (uses `list()` helper + client filter).
- **S-004 (T5)**: `phase.set` treats unknown `current_stage` as index 0 (`unwrap_or(0)`). This is safe for the forward-only rule, but consider emitting a warning/metric when a work has a non-canonical stage (may indicate data corruption or future phase extensions).
- **S-005**: All 9 handlers use typed `NexusApiError` variants with structured `field`/`reason` (or `resource`/`reason`). No raw-string control flow errors observed — good.
- **S-006 (cross-cutting)**: The 9 new handlers follow the existing `execute_*` + `registry_*` wrapper pattern and admission pipeline. Ownership is consistently enforced via `works::get_work(creator_id, work_id)` → Forbidden on cross-creator. Input validation uses explicit `as_str`/`as_i64` with `INVALID_INPUT`. No obvious injection or auth bypasses.

## Source Trace
- Finding ID: W-001
- Source Type: manual code review + diff
- Source Reference: `git diff 578be523...95d3595c -- crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs` (lines 2147-2183); comparison to chapter_update tx at 1423-1461
- Confidence: High

- Finding ID: W-002
- Source Type: manual code review + grep for canonicalize
- Source Reference: host_tool_handlers.rs:2135 (write path), 738 (older fs guard pattern), grep results for "canonicalize"
- Confidence: High

- Finding ID: W-003
- Source Type: test review
- Source Reference: host_tool_handlers.rs test `workspace_paths_rejects_without_workspace`
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 6 |

**Verdict**: Request Changes

### Rationale
- No Critical (remote-exploitable security) findings.
- However, the explicit focus items in the assignment ("is the atomic write correct?" for T3) surface a real correctness/data-integrity issue (W-001). The rename is atomic on disk, but the DB metadata update is not coordinated, violating the "temp+atomic-rename" expectation set by the prior C-002 pattern and the plan.
- W-002 is a defense-in-depth gap for a path that writes user-controlled-size content under a DB-supplied path.
- Per `mstar-review-qc` gate: unresolved Warning findings that directly address assignment focus areas block `Approve`. Targeted fix + re-review of the same `qc2.md` is appropriate.

### Revalidation notes (for targeted re-review)
When fixes land:
- Re-check `execute_manuscript_write` for tx-wrapped rename + word_count update (or explicit documented exception + compensating logic).
- Verify a canonicalize+prefix guard (or documented rationale why DB trust is sufficient).
- Confirm the weak test is either strengthened or removed if the "no workspace" path is now exercised deterministically in the fixture.
- Re-run: `cargo test -p nexus-daemon-runtime --test host_tool_executor_tests` (or the specific manuscript_* and workspace_paths_* tests) + `cargo clippy --all -- -D warnings`.

## Additional Scope Notes
- T1/T2/T4/T8/T9 handlers: straightforward delegation + ownership checks; no path construction from user input; typed errors; admission via registry + pipeline. No issues found in primary review.
- `nexus.research.query`: direct id path uses `sqlx::query!` (safe); list path uses helper + client-side filter. No SQLi surface.
- `nexus.workspace.paths`: pure enumeration of hardcoded roots under `workspace_root`; rejects cleanly when uninitialized.
- `nexus.manuscript.phase.set`: canonical set + forward-only rule + `force` override implemented exactly as described in plan/spec. No bypass observed (current_idx from DB, new_idx from const table, comparison before write).
- Spec deltas (acp-capability-set.md, capability-registry.md, etc.) correctly flip the 9 rows from catalog-only → shipped and document the contracts + test vectors. No contradictions with code observed.
- Registry count test and `catalog_registry_invariant_all_ids_present` updated to 30 and now assert bidirectionally for the shipped set (good).
- Legacy outbox deprecation note added in schema test (tracing::warn) — out of primary security scope for this review but noted as consistent with plan.
