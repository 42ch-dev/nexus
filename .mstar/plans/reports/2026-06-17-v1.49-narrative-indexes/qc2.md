---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-17-v1.49-narrative-indexes
verdict: Approve
generated_at: 2026-06-16T22:45:00Z
review_range: 3630a4e5..f448b658
working_branch: iteration/v1.49
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (file path safety, atomic writes, parser robustness, prompt injection provenance, error propagation, creator scoping, negative-path handling)
- Report Timestamp: 2026-06-16T22:45:00Z

## Scope
- plan_id: 2026-06-17-v1.49-narrative-indexes
- Review range / Diff basis: 3630a4e5..f448b658 (equivalent to `git diff 3630a4e5...f448b658`)
- Working branch (verified): iteration/v1.49
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 13 (4 feature commits + merge per `git log --oneline 3630a4e5..f448b658`)
- Commit range: 9e73a047 (parser/serializer/id/promote), 2ef52406 (wire into produce path), 01425b8c (sync_module skip regression), 1f037243 (completion report + residuals), f448b658 (merge)
- Tools run: `cargo check -p nexus-orchestration` (clean), `cargo test -p nexus-orchestration --lib narrative_index` (25/25 passed), `git diff --stat`, `git log --oneline`, file reads + targeted greps on parser, promotion, supervisor hook, preset injection, and prompt templates.

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-1 (Markdown table parser — embedded delimiter & malformed rows)**: `parse_foreshadowing_index` (via `parse_table` + `split_row`) uses a simple `trim_matches('|').split('|')` with padding/truncation to exactly 5 columns. A data row containing a literal `|` inside any cell (most critically the Description) produces extra cells; subsequent columns (Planted / Paid off / Status) are shifted or defaulted to empty. Wrong column counts (<5 or >5 pipes) are silently padded with empty strings or have trailing empties dropped — never rejected as malformed. Status is stored verbatim with no validation against the allowed vocabulary (`planned | buried | paid_off`). The parser is intentionally tolerant of "surrounding prose" from the scaffold template, but this tolerance also accepts user-edited or LLM-written rows that corrupt the index. The index is the SSOT for F### identity and is later emitted (via `read_foreshadowing_summary`) into outline/draft prompts for consistency. Corrupted rows lead to wrong id reuse, lost status, or garbage in the prompt summary. Existing tests cover empty files, full well-formed tables, placeholder-row skip, scaffold verbatim, and roundtrips; none exercise embedded `|`, column drift, control characters in cells, or non-canonical line endings. (Primary sources: `narrative_index.rs:231` (`split_row`), `203` (`parse_table` loop), `126` (`parse_foreshadowing_index`), `211` (`is_table_row`), `579` (`read_foreshadowing_summary` caller), `517` (promotion path); tests at `608-653` and `910-917`.)

- **W-2 (Atomic write — deterministic temp name, no O_EXCL/PID/random)**: `atomic_write` (used by `promote_outline_to_index`) writes to `<index_path>.md.tmp` (via `set_extension`) then `std::fs::rename`. The temp path is fully deterministic; there is no PID, UUID, `tempfile::NamedTempFile`, or `O_EXCL`/`create_new` guard. If two concurrent calls to `promote_outline_to_index` target the same `foreshadowing.md` (possible under daemon restart during a running schedule, multiple `nexus42` processes sharing a workspace, or any future relaxation of the per-Work single-driver assumption), the writers race on the shared `.tmp` file contents; the last rename wins and prior allocations or row data can be lost or produce a partially-written table. The module docs (`34-40`) and completion report explicitly defer an advisory lock and rest on the "single-writer daemon model" documented for `NovelProjectScaffold`. The supervisor hook (`on_schedule_terminal`) and `promote_foreshadowing_for_schedule` / `promote_outlines_in` are best-effort + non-fatal (warn + continue), so a torn write would not abort the schedule but would leave the index in an inconsistent state for subsequent chapters. Same pattern exists in sibling files (`rules_layers.rs`, `completion_lock.rs`) but those have different update cadences. (Primary sources: `narrative_index.rs:557-563` (`atomic_write`), `552` (call from promote), `507` (promote_outline_to_index), `478` (path helper); `auto_chain.rs:1217` and `1181` (promote_outlines_in), `459` (supervisor call site); `supervisor.rs:451-467` (non-fatal wrapper); completion.md §"Schema decision" + "Risks/follow-ups".)

### 🟢 Suggestion
- **S-1 (Conflicting-description error leaks prior description)**: When `promote_outline_to_index` detects an explicit `F###` id already present with a different description, it does `anyhow::bail!(..., row.description, decl.description)`. The error is caught in `promote_outlines_in` and turned into a `tracing::warn!` (non-fatal, schedule continues). The prior description (user narrative seed) is emitted verbatim in the log line. While this is not a secret, it is unnecessary leakage of the existing index content on every conflict. The message tells the operator to "reconcile the outline or edit foreshadowing.md manually", which is the correct recovery, but the log surface includes the full prior text. (Source: `narrative_index.rs:528-534`; consumption at `auto_chain.rs:1230-1241`.)

- **S-2 (Negative-path test coverage gaps for parser)**: The 25 `narrative_index` unit tests are strong on positive paths, idempotency, allocation, conflicting-id errors, atomic tmp cleanup, noop mtime preservation, and scaffold fidelity. They do not include cases for:
  - Embedded `|` (or other table delimiter) inside Description/Status.
  - Rows with 3, 4, 6, or 7 pipe cells (column drift).
  - Control characters, NUL bytes, or non-UTF8 (the latter fails at `read_to_string` before reaching the parser).
  - Oversized descriptions, alternate line endings (`\r\n`), or trailing whitespace-only data rows.
  - Status values outside the documented set (accepted verbatim).
  The parser's design choice ("tolerate surrounding prose") is intentional, but the delimiter-collision and column-count cases are user-visible integrity bugs for a file that is both hand-editable and LLM-augmented. Adding even a few "rejects or at least logs on obvious malformation" tests would make the tolerance boundary explicit.

- **S-3 (work_dir / workspace_dir provenance — defense in depth)**: `promote_outline_to_index`, `read_foreshadowing_summary`, and the parallel `build_preset_input` path for `foreshadowing_summary` all take a `work_dir` (or reconstruct it as `Path::new(ws_dir).join("Works").join(wref)`) with no internal `canonicalize`, `..` component stripping, or "must be descendant of workspace/Works/" assertion. The actual provenance is:
  - Schedule hook path: `supervisor.workspace_dir` (injected at `ScheduleSupervisor::new_with_workspace`) → `promote_foreshadowing_for_schedule` → `works::get_work(creator_id, work_id)` (enforces creator ownership) → `work.work_ref`.
  - Stage-gate path (CLI/daemon `stage advance`): `WorkFields.workspace_dir` + `work_ref` (caller-provided).
  This is identical to the pre-existing `rules_content` / `read_rules_layers` contract for the same per-Work tree. Creator scoping via the DB lookup prevents cross-creator promotion. However, if a future code path (or a compromised caller) passes an attacker-controlled `workspace_dir` or `work_ref` that contains `..`, the index read/write could escape the intended Work tree and either read another creator's foreshadowing or (less likely) write into it. Adding a small guard at the narrative_index layer (or at least a debug assertion) would reduce blast radius without changing the current trust model. No traversal is exploitable today via the reviewed call sites.

- **S-4 (Status enum is free-form string)**: The `ForeshadowingRow.status` field and the overlay vocabulary (`planned | buried | paid_off`) are documented but never enforced at parse or serialize time. Any string is accepted and round-tripped. This is not a security issue, but it is a latent correctness hole if future code (e.g. state machine transitions or UI) assumes one of the three values. (Source: `narrative_index.rs:82` (struct), `139` (parse), `277` (serialize); no validation site in the diff.)

## Source Trace
- Finding W-1: `narrative_index.rs:126-249` (parser), `579-596` (summary read), `517` (promotion caller), unit tests `608-653` + `910-917`.
- Finding W-2: `narrative_index.rs:557-563` (atomic_write), `507-554` (promote), `34-40` (module doc), `auto_chain.rs:1102-1182` (hook), `supervisor.rs:443-468` (non-fatal terminal hook).
- Finding S-1: `narrative_index.rs:528-534` (bail), `auto_chain.rs:1230-1241` (warn wrapper).
- Finding S-3: `stage_gates.rs:200-212` (parallel rules_content pattern), `auto_chain.rs:1149-1170` (get_work + join), `supervisor.rs:456-458` (ws_path threading).
- All other items cross-checked against `preset.yaml:93,117`, `outline-chapter.md:61-65`, `draft-chapter.md:48-54`, `sync_module.rs` skip invariant, and `e2e_novel_writing.rs` seed update (no new attack surface).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

(The two Warnings identify real correctness risks for index integrity — embedded-delimiter row corruption and deterministic-temp collision under violated single-writer assumptions — but both are low-to-medium impact for the local-first, single-daemon MVP, are mitigated by the documented "single-writer per Work" model and best-effort/non-fatal hook contract, and sit in the same trust boundary as the pre-existing `rules_content` / per-Work file handling. No Criticals. No high-impact unresolved trade-offs that would require `Needs Discussion`. The prompt-injection surface for `foreshadowing_summary` is intentional and identical in provenance handling to `rules_content`; it is not a new injection vector. Negative-path gaps are real but do not rise to blocking for this slice.)

## Completion Report v2

**Agent**: qc-specialist-2  
**Task**: QC2 (security/correctness) tri-review for plan 2026-06-17-v1.49-narrative-indexes  
**Status**: Done  
**Scope Delivered**: Verified cwd/branch/HEAD (iteration/v1.49 @ d78d240b), confirmed review range via `git diff 3630a4e5...f448b658 --stat` and `git log`, read all 13 changed files + focused inspection of parser, promotion, atomic write, supervisor hook, stage-gate injection, preset templates, and creator-scoping paths. Ran `cargo check` (clean) and the 25 narrative_index unit tests (all pass). Produced structured findings against the 10 explicit focus items in the Assignment. De-duplicated scope: no `qc1.md` or `qc3.md` present at report time (only `completion.md`); report is strictly security/correctness per role.  
**Artifacts**: `.mstar/plans/reports/2026-06-17-v1.49-narrative-indexes/qc2.md` (this file)  
**Validation**: All 10 focus areas (path safety, atomic-write collision, malformed-row tolerance, header matching, conflicting-description leakage, prompt-injection provenance vs rules_content, creator scoping via get_work, single-writer race + deferred lock, negative-path test gaps, error propagation/non-fatal hook) were exercised against source. No Criticals surfaced.  
**Issues/Risks**: Two Warnings (parser delimiter collision; deterministic temp name) and four Suggestions recorded. All are acceptable for local-first MVP under the documented assumptions; none block the current slice.  
**Plan Update**: None required from QC2 (residuals R-V149P1-01/02 already registered by implementer; no new residuals proposed).  
**Handoff**: Ready for PM consolidation with qc1 + qc3. If sibling reviewers surface overlapping items, PM should fold into consolidated report and status.json as needed.  
**Git**: (will be populated after commit)  
