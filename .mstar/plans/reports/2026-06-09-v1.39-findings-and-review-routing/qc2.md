---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-09-v1.39-findings-and-review-routing"
verdict: "Approve"
generated_at: "2026-06-09"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security and correctness risk (creator isolation, SQL injection surface, T3 hook trustworthiness, routing enum, CLI output safety, migration hygiene)
- Report Timestamp: 2026-06-09

## Scope
- plan_id: 2026-06-09-v1.39-findings-and-review-routing
- Review range / Diff basis: merge-base: 111c3611 + tip: 137fefaf; equivalent to `git diff 111c3611...137fefaf` (run in the Review cwd). 5 commits, 14 files, +1337 +10 / -4.
- Working branch (verified): feature/v1.39-findings-and-review-routing
- Review cwd (verified): .worktrees/v1.39-p1 (read-only)
- Files reviewed: 15 (per `git diff --stat`)
- Commit range: 111c3611...137fefaf (verified via `git log 111c3611..137fefaf --oneline` and `git rev-parse`)
- Tools run:
  - `git rev-parse --show-toplevel`, `git branch --show-current`, `git log ...`, `git diff --stat`, `git merge-base`
  - Full diff capture (`git diff 111c3611...137fefaf`)
  - `cargo clippy -p nexus-local-db -p nexus42 -p nexus-daemon-runtime -- -D warnings`
  - `cargo test -p nexus-daemon-runtime --test findings_api`
  - `cargo test -p nexus-orchestration --test auto_chain`
  - `cargo test -p nexus-orchestration --lib -- research`
  - `cargo test -p nexus-local-db --lib -- work_chapters`
  - `cargo +nightly fmt --all -- --check`
  - Context reads: plan, iteration compass v1.39, novel-writing/quality-loop.md, novel-writing/workflow-profile.md §5.5, key source (DAO, handlers, migration, CLI status, acp_worker)

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none — all security/correctness items below Acceptable or Suggestion)

### 🟢 Suggestion
- **T3 hook (from-review) trustworthiness / severity floor**: `create_from_review_handler` correctly resolves `creator_id` from authenticated context (never from body) and performs work ownership check via `works::get_work(creator, work_id)`. The `ReviewVerdictFinding` struct is a thin carrier; `create_finding_from_review` still forces `status="open"` + fresh ULID + timestamps. However, there is no severity floor or triviality filter in this minimal path slice — an LLM judge outputting "blocker" for a trivial issue can create a `blocker` finding. Per assignment explicit note and plan scope (P1 = minimal path; policy/enforcement in P2 presets + rules), this is acceptable for gate. Observation only: add floor or classification step before P2 to prevent noisy blocker spam.
  - Source Trace: Finding ID: S-001; Source Type: manual-reasoning + diff (handlers/findings.rs: create_from_review_handler + ReviewVerdictFinding); Source Reference: `git diff ... crates/nexus-daemon-runtime/src/api/handlers/findings.rs:190-220` + `nexus-local-db/src/findings.rs:310-340`; Confidence: High
- **CLI status section title/description safety (potential future ANSI)**: T5 prints `title`, `severity`, `chapter`, `routing_hint` via `println!` + simple format. Titles originate from API (JSON body for direct create, or review verdict for from-review). Current code does not strip or escape control sequences; a crafted title containing `\x1b[..."` would be emitted literally (terminal injection surface). No evidence of deliberate crafting or raw interpolation in this change; titles are short human labels per spec. For P1 display-only UX this is low risk, but add sanitization (strip ANSI or use safe writer) in follow-up.
  - Source Trace: Finding ID: S-002; Source Type: manual-reasoning + diff; Source Reference: `crates/nexus42/src/commands/creator/run.rs:718-760` (the Findings block); Confidence: Medium
- **Migration + DAO hygiene (minor polish)**: Migration uses `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` + correct FK `ON DELETE CASCADE` to works; all DAO queries are `sqlx::query!` (compile-time, parameters bound, no string interpolation for enums or LIKE — no LIKE present). `count_open_findings_by_severity` correctly falls back to runtime query with `// SAFETY` comment for COUNT(*) inference. All good; no action required for this gate.
  - Source Trace: Finding ID: S-003; Source Type: manual-reasoning + diff + spec cross-check; Source Reference: `crates/nexus-local-db/migrations/202606090002_findings.sql` + `src/findings.rs:78-96` (create), `98-140` (list with NULL guards), `200-250` (update), `310-340` (create_from_review); Confidence: High
- **PM fix wave (positive note, no penalty)**: The final commit `fix(nexus42,v1.39-p1): PM fix wave — dedupe session_captures + map_or_else clippy` (unrelated to findings) cleaned real defects (session_captures double-decl + clear-on-init, map_or_else clippy). Assignment explicitly says "Note them but don't penalize the gate." Correctly observed; the findings surface is unaffected and tests still pass.
  - Source Trace: Finding ID: S-004; Source Type: git log + diff; Source Reference: last commit in `git log 111c3611..137fefaf`; Confidence: High

## Source Trace
(See individual findings above for per-item traces. All traces cross-reference the exact diff hunks, handler paths, DAO query! sites, migration, and spec §2 / §5.5.6.)

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 (all non-blocking for P1 minimal path; 1 is positive PM note) |

**Verification evidence** (all run in Review cwd on the exact review range):
- `cargo clippy -p nexus-local-db -p nexus42 -p nexus-daemon-runtime -- -D warnings` → clean (0 errors after tail)
- `cargo test -p nexus-daemon-runtime --test findings_api` → 7/7 passed (exactly the creator isolation, CRUD, from-review, routing-hints, delete, list-filter tests)
- `cargo test -p nexus-orchestration --test auto_chain` → 21 passed (no driver fork regression)
- `cargo test -p nexus-orchestration --lib -- research` → 17 passed
- `cargo test -p nexus-local-db --lib -- work_chapters` → 21 passed
- `cargo +nightly fmt --all -- --check` → clean (no output)
- Git alignment: cwd, branch, 5 commits, diff stats all match Assignment (minor stat variance from PM fix wave noted)

**Verdict**: Approve

## Branch / Scope Discipline Confirmation
- Zero source-file modifications performed by this reviewer.
- Zero subagent / Task / delegation invocations (per qc-specialist-2 NEVER rules + assignment "Do NOT delegate").
- All review activity executed inside the assigned Review cwd `.worktrees/v1.39-p1` on the assigned `Working branch`.
- Report written only under `{PLAN_DIR}/reports/<plan-id>/qc2.md` (no other paths touched).
- PM fix wave (dedupe + clippy) noted positively per explicit instruction; does not affect findings security/correctness surface.

**Creator isolation (qc2 primary focus)**: All public endpoints and the from-review hook resolve `creator_id` exclusively from `read_active_creator_id(state.nexus_home())` (request context / active config). No `creator_id` field exists in any request body (CreateFindingRequest, UpdateFindingRequest, ListFindingsQuery, or the from-review body). Every DAO call is passed the context-resolved `creator_id`; cross-creator GET/list returns 404/empty (covered by dedicated test `findings_creator_isolation_cross_creator_404`). Work ownership is re-verified on get/update/delete/from-review paths. **Pass**.

**SQL injection / enum surface**: All production queries use `sqlx::query!` (compile-time checked). Enum columns (`severity`, `status`, `target_executor`) are bound as parameters (`?`), never interpolated. Optional filter guards use `? IS NULL OR col = ?` (standard safe pattern). No `LIKE` or dynamic string construction in this change. Migration is pure DDL with IF NOT EXISTS. **Pass**.

**T3 hook + routing enum + migration + CLI safety**: See Suggestion items above (mitigated for P1 scope). **Acceptable for this gate**.

**Regression / maintainability / branch discipline**: All required test suites pass; clippy + nightly fmt clean; changes are surgical (new findings module + handlers + one status section + migration + sqlx metadata); executed inside assigned worktree/branch with no source edits. **Pass**.
