---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-13-v1.44-manuscript-audit-preset"
verdict: "Request Changes"
generated_at: "2026-06-13"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-13T18:00:00Z

## Scope
- plan_id: 2026-06-13-v1.44-manuscript-audit-preset
- Review range / Diff basis: 068135ed..9d471bdc
- Working branch (verified): iteration/v1.44
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 6
- Commit range: 83905581..bce2e81a (5 commits) merged at 9d471bdc
- Tools run: cargo clippy -p nexus42 -p nexus-orchestration -- -D warnings, cargo +nightly fmt --all --check, cargo test -p nexus-orchestration --test novel_manuscript_audit, cargo test -p nexus42 -- run::tests

## Findings

### 🔴 Critical

*None.*

### 🟡 Warning

- **W1 — Preset YAML routing: `load_chapter` hardcodes `next: review_report` but extract path is implicit** (maintainability)
  The preset YAML declares `load_chapter.next: review_report` as a single linear transition, yet the spec (§3.1, §3.2) defines two distinct paths: `load_chapter → review_report` (review mode) and `load_chapter → extract_sync` (extract mode). The `extract_sync` state has no incoming transition declared in the YAML — it is reachable only through runtime mode-based branching at `load_chapter`'s `exit_when: kind: manual`. This implicit routing is a maintainability risk: a future engineer reading the YAML in isolation would not understand that `extract_sync` is reachable. The YAML should either (a) document the runtime branching behavior in a comment on `load_chapter.next`, or (b) adopt a conditional-next form when the orchestrator supports it.
  → Add a YAML comment on `load_chapter.next` explaining that the runtime branches to `extract_sync` when `mode=extract`, or split into two presets if conditional routing is not expressible in the current YAML grammar.

- **W2 — `resolve_audit_body_path` accepts but ignores `volume` parameter** (maintainability)
  The function signature at `run.rs:line ~880` takes `volume: i32` but prefixes it as `_volume` (unused). The spec §3.1 states `volume` is "required when Work has multi-volume chapters," but the body_path resolution does not filter by volume. This is misleading: a caller passing `volume=2` would get the first matching chapter row regardless of volume. While multi-volume body_path resolution is deferred to P2, the unused parameter creates a false sense of correctness.
  → Either remove the `volume` parameter from `resolve_audit_body_path` (and add a `// TODO(P2): filter by volume` comment), or implement volume-aware lookup now. The current signature is a latent bug vector.

- **W3 — Plan §6 verification command references nonexistent test file** (correctness/docs)
  The plan §6 lists `cargo test -p nexus42 --test audit_chapter_cli` as a verification command, but no `audit_chapter_cli` integration test file exists. The actual audit-chapter CLI tests live in `run.rs` unit tests (`run::tests` module). Running the plan's command verbatim would fail with "no such test target." This is a documentation drift between plan and implementation.
  → Update plan §6 to reference the actual test locations: `cargo test -p nexus42 -- run::tests` and `cargo test -p nexus-orchestration --test novel_manuscript_audit`.

### 🟢 Suggestion

- **S1 — `handle_audit_chapter` is ~90 lines with a clippy allow** (maintainability)
  The function carries `#[allow(clippy::too_many_lines)]` with a justification comment. While the justification ("splitting would create a >7-arg helper") is reasonable, the function could still benefit from extracting two focused helpers: `build_audit_schedule_input()` (~30 lines of JSON construction) and `print_audit_result()` (~15 lines of output formatting). This would bring the handler under the clippy threshold without creating unwieldy parameter lists.
  → Extract `build_audit_schedule_input()` and `print_audit_result()` helpers; remove the `#[allow]`.

- **S2 — Test duplication between `novel_manuscript_audit.rs` and `run.rs`** (maintainability)
  Three tests in `novel_manuscript_audit.rs` (`worldless_work_returns_422_on_extract`, `body_path_resolution_from_work_response`, `body_path_returns_none_for_missing_chapter`) are logic simulations that duplicate equivalent tests in `run.rs` (`resolve_audit_body_path_finds_chapter`, `resolve_audit_body_path_returns_none_for_missing`, etc.). The orchestration test file tests preset loading (correct scope), but the logic-simulation tests test CLI handler logic that is already covered in the `run.rs` unit tests. This creates a maintenance burden: changes to `resolve_audit_body_path` require updating tests in two files.
  → Remove the logic-simulation tests from `novel_manuscript_audit.rs` (keep only preset-loading and state-machine tests there). The `run.rs` unit tests are the canonical location for `resolve_audit_body_path` and worldless-extract logic.

- **S3 — `AddScheduleRequest` uses explicit `None` for all optional fields** (style)
  The schedule request construction sets `depends_on: None`, `concurrency: None`, `scheduled_at: None`, `force_gates: false`, `reason: None` explicitly. If `AddScheduleRequest` derives `Default`, these could be replaced with `..Default::default()` for conciseness.
  → Consider `#[derive(Default)]` on `AddScheduleRequest` or a builder pattern to reduce boilerplate in future CLI handlers.

- **S4 — `world_id` injected into audit input even for review mode** (clarity)
  In `handle_audit_chapter`, `world_id` is unconditionally inserted into `audit_input` when present, even for review mode. While harmless (the preset ignores unused fields), it adds noise to the schedule payload. A conditional insert (`if matches!(mode, AuditMode::Extract)`) would make the intent clearer.
  → Gate the `world_id` insert on `AuditMode::Extract` to make the data flow explicit.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W1 | manual-reasoning | `preset.yaml` lines 76, 95–109 (load_chapter.next vs extract_sync with no incoming transition) | High |
| W2 | git-diff | `run.rs` `resolve_audit_body_path(work_resp, chapter, _volume)` — `_volume` prefix | High |
| W3 | doc-rule | Plan §6 verification command vs actual test file layout | High |
| S1 | linter | `#[allow(clippy::too_many_lines)]` on `handle_audit_chapter` | Medium |
| S2 | manual-reasoning | Cross-file comparison: `novel_manuscript_audit.rs` lines 296–365 vs `run.rs` lines 1510–1545 | Medium |
| S3 | manual-reasoning | `run.rs` lines 775–785 (explicit `None` fields) | Low |
| S4 | manual-reasoning | `run.rs` lines 765–773 (unconditional `world_id` insert) | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

**Rationale**: Three Warning findings remain unresolved. W1 (implicit preset routing) is an architecture clarity issue that could cause confusion during P1/P2 extension. W2 (unused `volume` parameter) is a latent bug vector for multi-volume works. W3 (plan verification command mismatch) is a documentation correctness issue. Per `mstar-review-qc` gate rules, unresolved Warning findings prevent `Approve`.

**Positive observations**:
- All 14 preset tests + 5 CLI handler tests pass (0 failures).
- `cargo clippy` clean on both affected crates.
- `cargo +nightly fmt --all --check` clean.
- The preset YAML is well-structured with clear state descriptions and capability bindings.
- The CLI handler correctly enforces the worldless-extract 422 precondition.
- No FL-E driver fields leak into the preset (verified by `preset_does_not_set_fl_e_stage_driver_fields` test).
- The `AuditMode` enum with `Display` impl is clean and idiomatic.
- The `resolve_audit_body_path` helper is properly extracted and independently testable.
