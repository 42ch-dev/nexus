---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-13-v1.44-manuscript-audit-preset"
verdict: "Request Changes"
generated_at: "2026-06-13"
---

# Code Review Report — QC #3 (Performance / Reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk (DF-69 P0)
- Report Timestamp: 2026-06-13

## Scope
- plan_id: `2026-06-13-v1.44-manuscript-audit-preset`
- Review range / Diff basis: `068135ed..9d471bdc` (verbatim from Assignment)
- Working branch (verified): `iteration/v1.44`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 6 changed files in range
- Commit range: `068135ed..9d471bdc`
- Tools run:
  - `cargo clippy -p nexus42 -p nexus-orchestration -- -D warnings` — PASS
  - `cargo test -p nexus42 --lib` — PASS (640 passed)
  - `cargo test -p nexus-orchestration --test novel_manuscript_audit` — PASS (14 passed)
  - `cargo +nightly fmt --all --check` — PASS

## Findings

### 🔴 Critical

#### F-001: Extract mode is unreachable in the preset state machine
- **Issue**: `crates/nexus-orchestration/embedded-presets/novel-manuscript-audit/preset.yaml` declares `load_chapter.next: review_report` unconditionally. The `extract_sync` state exists but is never entered, so `mode=extract` schedules follow the review path (`load_chapter → review_report → done`) instead of invoking `kb.extract_work`.
- **Impact**: The extract-mode feature required by [novel-manuscript-audit.md](../../../../knowledge/specs/novel-manuscript-audit.md) §3.2 is completely non-functional. Users can request `--mode extract`, the CLI accepts it, but the daemon will run a review prompt instead.
- **Evidence**:
  - `preset.yaml` lines 60–76: `load_chapter.next: review_report` with no conditional branch.
  - `crates/nexus-orchestration/src/preset/loader.rs` rejects `NextTarget::Conditional` in V1.4 (`conditional next is not yet supported`), confirming there is no hidden branching mechanism.
  - The orchestration tests (`novel_manuscript_audit.rs`) verify `extract_sync` exists and calls `kb.extract_work`, but do **not** verify it is reachable from `load_chapter` based on `mode`.
- **Fix**: Introduce a mode-aware branch. Options:
  1. Use the supported GoNogo conditional next on `load_chapter` (if the engine allows non-`llm_judge` GoNogo) with a rule/capability that reads `preset.input.mode`.
  2. Split into two presets (`novel-manuscript-audit-review` and `novel-manuscript-audit-extract`) and have the CLI dispatch the correct one.
  3. Make `load_chapter` use `exit_when: rule` and branch to `extract_sync` when `mode == "extract"`.
- **Source Trace**: `preset.yaml:60-109`, loader.rs conditional-next rejection, tests `extract_sync_state_calls_kb_extract_work` and `extract_sync_state_transitions_to_done`.

### 🟡 Warning

#### F-002: CLI returns immediately for an operation advertised as synchronous/on-demand
- **Issue**: `handle_audit_chapter` in `crates/nexus42/src/commands/creator/run.rs` creates a schedule via `POST /v1/local/orchestration/schedules` and returns the schedule ID. The daemon endpoint inserts the schedule with status `pending` and returns (`schedules.rs:489-496`, `598-605`); execution is picked up asynchronously by the supervisor/worker.
- **Impact**: Review-mode users are told "Report will be written to Works/{work_ref}/Logs/review/" but receive no confirmation, completion status, or report path. Extract-mode users are told extraction "will run synchronously" but the CLI returns before `kb.extract_work` executes, contradicting the spec's "without queue ceremony" language and the on-demand user experience.
- **Evidence**: `run.rs:839-851` posts schedule and prints success message without waiting; `schedules.rs` returns `status: "pending"` in all paths.
- **Fix**: For truly synchronous on-demand behavior, either:
  - Add a daemon endpoint/capability path that executes `kb.extract_work` (and, for review, the report writer) inline and returns the result; or
  - Add a `--wait`/`--poll` flag and have the CLI poll schedule status/inspect core context until terminal, then surface the report path or extraction summary.
- **Source Trace**: `run.rs:695-877`, `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:88-606`.

#### F-003: `resolve_audit_body_path` ignores `volume`
- **Issue**: The helper matches only on `chapter` and discards the `_volume` parameter.
- **Impact**: For multi-volume Works where the same chapter number exists in multiple volumes, the helper may return the wrong `body_path` or fail to resolve the intended chapter. This directly conflicts with the spec's multi-volume support and the compass R-V142P1 hardening theme.
- **Evidence**: `run.rs:884-901` matches `c.get("chapter")` only.
- **Fix**: Match the tuple `(chapter, volume)` against the chapters array, or document that chapter numbers are globally unique (which contradicts `novel-workflow-profile.md`).
- **Source Trace**: `run.rs:884-901`, unit tests `resolve_audit_body_path_*`.

#### F-004: Missing `body_path` causes strict template-render failure at runtime
- **Issue**: `handle_audit_chapter` only inserts `body_path` into the preset input when it can be resolved from the Work's `chapters` array. If the array is absent or the chapter/volume is not found, `preset.input.body_path` is omitted. The preset then references `{{preset.input.body_path}}` in `load_chapter` vars, and the orchestrator renders capability args in strict mode (`render_value_templates` → `render_strict_template`), which fails on missing keys.
- **Impact**: A common case (new Work with no chapters row yet, or a typo in `--chapter`) results in a schedule that fails at runtime with a template-render error rather than a clear precondition error. This degrades reliability and error observability.
- **Evidence**: `run.rs:778-785` only sets `body_path` when `Some`; `crates/nexus-orchestration/src/tasks/mod.rs:1398-1408` uses strict mode for capability arg templates.
- **Fix**: Always insert `body_path` (e.g., fall back to the layout SSOT path `Works/{work_ref}/Stories/ch{chapter}.md`, or insert an empty string and handle it in the prompt). Alternatively, fail fast in the CLI with a clear message if the chapter cannot be resolved.
- **Source Trace**: `run.rs:778-785`, `preset.yaml:67-73`, `tasks/mod.rs:1398-1408`.

#### F-005: Worldless-extract 422 is not a structured error
- **Issue**: The CLI returns `CliError::Other(format!("422 world_required_for_extract: ..."))`. In `--json` mode this still emits a human string, not a machine-readable error object with a stable code.
- **Impact**: Downstream automation cannot reliably detect the `world_required_for_extract` precondition failure. The spec names the error `422 world_required_for_extract`, but the implementation does not expose it as a structured field.
- **Evidence**: `run.rs:759-766` builds a free-text string.
- **Fix**: Add a dedicated `CliError` variant (or use a structured JSON error in `--json` mode) carrying `code: "world_required_for_extract"`, `work_id`, and a remediation link.
- **Source Trace**: `run.rs:759-766`.

#### F-006: No end-to-end CLI integration test for `audit-chapter`
- **Issue**: The plan's verification block lists `cargo test -p nexus42 --test audit_chapter_cli`, but no such test file exists in the diff. The existing tests are limited to unit tests of `AuditMode` display/`resolve_audit_body_path` in `run.rs` and preset-structure tests in `nexus-orchestration`.
- **Impact**: The hot path (CLI arg parsing → daemon request → schedule creation → error handling) is not exercised under test. Regressions in the request shape, JSON output, or 422 path will not be caught by CI.
- **Evidence**: `git diff --stat 068135ed..9d471bdc` shows no `crates/nexus42/tests/audit_chapter_cli.rs`; plan §6 lists the missing command.
- **Fix**: Add an integration test that mocks the daemon client and asserts request shape, JSON response keys (`audit_mode`, `chapter`, `volume`), and the worldless-extract error path.
- **Source Trace**: `run.rs:128-187`, `run.rs:695-877`, plan §6.

#### F-007: Runtime lock invariant is not explicitly honored
- **Issue**: The spec §4 invariants state: "Command must respect `runtime_lock_holder` when Work is locked (same as other mutating `creator run` paths)." The handler issues `GET /v1/local/works/{work_id}` and then `POST /v1/local/orchestration/schedules`; it does not pass or check any lock context.
- **Impact**: If schedule creation is treated as a mutating operation by the daemon, concurrent `audit-chapter` invocations (or audit + stage advance) on the same Work could race. At minimum, the invariant is not visibly enforced in the CLI contract.
- **Evidence**: `run.rs:695-877` has no lock-related fields; comparison paths like `stage_advance` use `stage_gates::check_stage_advance` and runtime-lock-aware daemon APIs.
- **Fix**: Either confirm (and document in a comment) that daemon schedule creation serializes per-Work via `creator_schedules` concurrency, or acquire/pass the runtime lock through the daemon API before scheduling.
- **Source Trace**: `run.rs:695-877`, spec §4 invariants.

### 🟢 Suggestion

#### S-001: Report writing is not enforced by a capability call
- **Improvement**: Review mode relies on the LLM agent reading the prompt instructions and using a file-write tool to produce `Works/{work_ref}/Logs/review/audit-ch{nn}-v{vol}-{timestamp}.md`. There is no explicit capability invocation (e.g., `fs/write_text_file`) in the preset to guarantee the report is written, and no output binding to verify it.
- **Rationale**: This is consistent with the existing prompt-driven preset style (`novel-writing`), but for an on-demand audit it increases observability risk. Consider adding a follow-up state that invokes a structured report-persistence capability or records a schedule output binding.
- **Source Trace**: `preset.yaml:78-93`, `prompts/review-report.md`.

#### S-002: `run_intents: [work_continue]` is misleading for an audit preset
- **Improvement**: The preset declares `run_intents: - work_continue`. Auditing is not a "continue" action; for extract mode `knowledge_ingest` would be more accurate. Consider splitting intents per mode or using a dedicated `audit` intent.
- **Source Trace**: `preset.yaml:44-45`.

#### S-003: Seed format is not deterministic enough for idempotency
- **Improvement**: The schedule seed is `audit-chapter {work_id} mode={mode} ch={chapter} vol={volume}`. This is fine for display, but if the daemon ever deduplicates schedules by seed, note that `mode` uses the `Display` impl (lowercase). This is acceptable as-is but worth documenting.
- **Source Trace**: `run.rs:819-821`.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| F-001 | static-analysis + doc-rule | `preset.yaml:60-109`; `loader.rs` conditional-next rejection | High |
| F-002 | static-analysis + doc-rule | `run.rs:839-851`; `schedules.rs:489-496,598-605` | High |
| F-003 | static-analysis + manual-reasoning | `run.rs:884-901`; spec §3.1 | High |
| F-004 | static-analysis + manual-reasoning | `run.rs:778-785`; `tasks/mod.rs:1398-1408` | High |
| F-005 | static-analysis | `run.rs:759-766` | High |
| F-006 | static-analysis + manual-reasoning | diff stat; plan §6 | High |
| F-007 | doc-rule + manual-reasoning | spec §4 invariants; `run.rs:695-877` | Medium |
| S-001 | manual-reasoning | `preset.yaml:78-93` | Medium |
| S-002 | manual-reasoning | `preset.yaml:44-45` | Medium |
| S-003 | manual-reasoning | `run.rs:819-821` | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 6 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

The P0 implementation is clean structurally and passes lint/tests, but the preset state machine does not branch on `mode`, making `extract` mode unreachable. This is a ship-blocking correctness defect. Additionally, the CLI's async schedule-return path does not deliver the synchronous/on-demand experience described in the spec, and several reliability gaps (volume-aware lookup, missing-body-path strict-render failure, unstructured 422, missing CLI integration test, runtime-lock invariant) should be addressed or explicitly dispositioned before merge.
