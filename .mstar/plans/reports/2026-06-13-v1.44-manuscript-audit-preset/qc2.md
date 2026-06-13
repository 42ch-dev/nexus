---
report_kind: qc-report
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-13-v1.44-manuscript-audit-preset"
verdict: Request Changes
generated_at: "2026-06-13T12:10:00+08:00"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (extract-mode World-bound precondition, daemon API input validation, chapter body path resolution, audit-mode upsert hook safety, prompt-injection surfaces)
- Report Timestamp: 2026-06-13T12:10:00+08:00

## Scope
- plan_id: 2026-06-13-v1.44-manuscript-audit-preset
- Review range / Diff basis: 068135ed..9d471bdc
- Working branch (verified): iteration/v1.44
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 6
- Commit range: 83905581..bce2e81a (5 commits) + merge at 9d471bdc
- Tools run:
  - `git log --oneline 068135ed..9d471bdc` (reproduced 5 commits + merge)
  - `git diff 068135ed..9d471bdc -- <files>` (run.rs, preset.yaml, prompts/*, novel_manuscript_audit.rs)
  - `cargo clippy -p nexus42 -p nexus-orchestration -- -D warnings` (clean)
  - `cargo test -p nexus-orchestration --test novel_manuscript_audit` (14 passed)
  - `cargo test -p nexus42 --lib` (module tests for AuditMode + resolve_audit_body_path) + `review_master_cli` (related surface tests, all green)
  - `git log -1 --oneline` (post-commit)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **F-QC2-001 (Correctness / Security boundary — World-bound extract precondition)**: The `422 world_required_for_extract` check for `mode=extract` on a worldless Work lives **only** in the CLI handler `handle_audit_chapter` (run.rs:712–718) **before** the `AddScheduleRequest` is built. The generic daemon schedule creation endpoint (`/v1/local/orchestration/schedules`) and the `novel-manuscript-audit` preset itself (preset.yaml: `world_binding: { mode: optional }`, `extract_sync` state) perform no re-validation. A direct `daemon schedule add --preset novel-manuscript-audit --input '{"mode":"extract", "work_id":"...", "chapter":N, "world_id":null}'` (or an older/alt client) can bypass the documented precondition in spec §3.2 and invoke `kb.extract_work` on a Worldless Work. The preset `extract_sync` state blindly forwards whatever `world_id` (or absence) is present in the schedule input to the capability.
  - Source: `crates/nexus42/src/commands/creator/run.rs` (handle_audit_chapter + world_id extraction), `crates/nexus-orchestration/embedded-presets/novel-manuscript-audit/preset.yaml` (world_binding + extract_sync), orchestration test `worldless_work_returns_422_on_extract` (only simulates the CLI logic).
  - Impact: Violates the explicit "Worldless Works receive `422 world_required_for_extract`" contract; risk of incorrect KB state or cross-scope data handling.
  - Fix: Add a gate (or pre-state validation) in the preset manifest / orchestration engine for this preset when `mode=extract`, or have the schedule-add path for known audit presets re-enforce the world_id presence. At minimum, make `extract_sync` fail closed if `world_id` is absent.

- **F-QC2-002 (Correctness / Path handling — body_path resolution into kb.extract_work)**: `resolve_audit_body_path` (run.rs:780–790) performs a simple `chapters[].find(chapter)` lookup on the response from `/v1/local/works/{work_id}` and copies the raw `body_path` string verbatim into the schedule `input.body_path` (and subsequently `source_locator` for the `kb.extract_work` capability in `extract_sync`). There is **no** canonicalization, no check that the path is relative to the Work's `Works/<work_ref>/Stories/` tree, no rejection of `..`, absolute paths, or control characters. The stored `body_path` values originate from manuscript creation paths (outside this P0 diff). The same value flows into the `load-chapter.md` prompt template.
  - Source: `crates/nexus42/src/commands/creator/run.rs` (resolve_audit_body_path and the audit_input construction), preset `extract_sync` args, `prompts/load-chapter.md`.
  - Impact: If a `body_path` in a Work record is ever malformed or attacker-influenced (historical or via another surface), the on-demand extract path can be made to reference files outside the intended workspace layout. This is a latent path-traversal / file-disclosure vector at the capability boundary.
  - Fix: Normalize the resolved path against the expected layout (e.g. must start with `Works/{work_ref}/Stories/` and contain no `..` after normalization) before passing to the preset / `kb.extract_work`. Consider making the preset reject suspicious `source_locator` values for `source_kind: "work_chapter"`.

### 🟢 Suggestion
- **F-QC2-003 (Prompt surface — template variable interpolation)**: Both `prompts/load-chapter.md` and `prompts/review-report.md` interpolate `{{work_ref}}`, `{{work_id}}`, `{{body_path}}`, `{{chapter}}`, `{{volume}}` (and `upsert_findings`) directly via `creator.inject_prompt` capability. While the current templates are narrow and the variables are mostly structural IDs, `work_ref` and `body_path` are user-visible strings that could contain newlines, markdown, or (in future) narrative content. No escaping or allow-listing is applied at the template layer.
  - Source: `crates/nexus-orchestration/embedded-presets/novel-manuscript-audit/prompts/*.md`, preset.yaml `enter` actions for `load_chapter` and `review_report`.
  - Recommendation: Document the trust boundary for these variables in the preset and/or `novel-manuscript-audit.md`. For defense-in-depth, consider basic sanitization (strip control chars, limit length) when building the vars map in the CLI handler, or add a note that model output from these prompts must be treated as untrusted when written to `Logs/review/`.

- **F-QC2-004 (Audit-mode upsert hook safety — visibility gap)**: Review mode hard-codes `"upsert_findings": true` in the schedule input (run.rs:728). The `review_report` state only invokes `creator.inject_prompt` with the flag as a template var; the visible preset state machine contains **no** explicit findings-upsert capability step. The actual upsert (if performed) must occur downstream (in the agent execution of the generated report, a post-done hook, or inside the `inject_prompt` implementation for this preset). No test in the P0 scope asserts that any created findings are correctly scoped to `(work_id, chapter, volume)` and cannot leak across Works.
  - Source: CLI handler construction of `audit_input`, preset.yaml `review_report` state (only the prompt), absence of a findings-related capability in the state graph.
  - Recommendation: Either add an explicit (scoped) findings-upsert capability step after the review prompt in the preset, or add a hermetic test that the review execution path produces findings rows (or a mock) with the expected `target_work` / chapter scoping. Confirm the behavior in `novel-quality-loop.md` §2.2 is honored for this on-demand path.

- **F-QC2-005 (Daemon API input validation — generic schedule surface)**: The `AddScheduleRequest` sent by the audit handler uses an open `input: Some(audit_input)` object. No schema or preset-specific validation of the audit fields (`mode`, `chapter`, `volume`, `world_id` presence for extract) occurs at the generic `/v1/local/orchestration/schedules` boundary in this change set. The preset manifest provides `requires_capabilities` and `gates` (work_profile + work_ref), but these do not encode the mode-dependent world requirement.
  - Source: `run.rs` (AddScheduleRequest construction), preset.yaml (gates + world_binding).
  - Recommendation: For embedded presets with mode-dependent security preconditions, either (a) extend the preset manifest with an input schema / gate kind that the scheduler can enforce before enqueueing, or (b) document that the primary documented entry point (the `nexus42 creator run audit-chapter` CLI) is the only supported path and direct schedule creation for this preset is unsupported / at-own-risk.

## Source Trace
- Finding F-QC2-001: git-diff (run.rs handle_audit_chapter world_id check), preset.yaml (world_binding + extract_sync), spec `novel-manuscript-audit.md` §3.2, orchestration test `worldless_work_returns_422_on_extract` (client-side simulation only). Confidence: High.
- Finding F-QC2-002: git-diff (resolve_audit_body_path + body_path insertion into audit_input), preset extract_sync source_locator wiring, entity-scope-model.md (World-bound semantics). Confidence: High.
- Finding F-QC2-003: git-diff (prompt templates), preset.yaml enter actions for inject_prompt. Confidence: Medium.
- Finding F-QC2-004: git-diff (CLI hard-coded upsert_findings + preset state machine lacking explicit upsert step), plan AC and spec §3.1. Confidence: Medium.
- Finding F-QC2-005: git-diff (generic AddScheduleRequest), preset.yaml gates section. Confidence: High.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

Per gate rules: presence of unresolved `Warning` items (security/correctness boundary enforcement and path handling) blocks `Approve`. The primary documented CLI entry point enforces the worldless-extract rule and tests are green, but the generic daemon schedule surface and preset do not re-enforce the documented contract. Path resolution for `body_path` → `kb.extract_work` lacks normalization. These must be addressed (or explicitly accepted with residual + justification) before this P0 can merge to `main`.

---

**Evidence (reproduced in this session)**

```bash
# Diff basis (exact)
git log --oneline 068135ed..9d471bdc
# 9d471bdc merge(v1.44 P0): manuscript-audit preset + CLI entry
# bce2e81a feat(v1.44): T7 — amend cli-spec.md §6.2D with audit-chapter IA
# 6428ba15 feat(v1.44): T6 — audit tests + structural fixes
# 97321916 feat(v1.44): T4+T5 — review mode report + extract mode sync wiring
# 863e2069 feat(v1.44): T3 — audit-chapter CLI handler + daemon schedule wiring
# 83905581 feat(v1.44): T1+T2 — novel-manuscript-audit embedded preset + prompts

cargo clippy -p nexus42 -p nexus-orchestration -- -D warnings   # clean
cargo test -p nexus-orchestration --test novel_manuscript_audit  # 14 passed
cargo test -p nexus42 --lib  # relevant AuditMode + resolve_* tests + others green
cargo test -p nexus42 --test review_master_cli  # related CLI surface green
```

(End of report)
