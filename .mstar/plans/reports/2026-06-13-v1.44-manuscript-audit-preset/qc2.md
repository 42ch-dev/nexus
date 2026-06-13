---
report_kind: qc-report
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-13-v1.44-manuscript-audit-preset"
verdict: "Approve"
generated_at: "2026-06-13T12:40:00+08:00"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (extract-mode World-bound precondition, daemon API input validation, chapter body path resolution, audit-mode upsert hook safety, prompt-injection surfaces)
- Report Timestamp: 2026-06-13T12:40:00+08:00

## Scope
- plan_id: 2026-06-13-v1.44-manuscript-audit-preset
- Review range / Diff basis: 9d471bdc..44a12a6e (targeted fix-wave re-review)
- Working branch (verified): iteration/v1.44
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: P0 fix surfaces (CLI handler + error variant, two split presets + prompts, orchestration test modules for review/extract; full wave diff 12 files, scoped to security/correctness surfaces per Assignment)
- Commit range: d6b9400e (split + dispatch), 3297d925 (CLI hardening), fc9f2f6d (plan tests), 44a12a6e (merge)
- Tools run:
  - `git log --oneline 9d471bdc..44a12a6e` (reproduced 3 fix commits + merge)
  - `git diff 9d471bdc..44a12a6e -- <scoped files>` (run.rs, errors.rs, novel-manuscript-audit-*/preset.yaml, novel_manuscript_audit*.rs)
  - `cargo +nightly fmt --all --check` (clean)
  - `cargo clippy --all -- -D warnings` (clean)
  - `cargo test -p nexus-orchestration --test novel_manuscript_audit --test novel_manuscript_audit_review --test novel_manuscript_audit_extract` (14 + 7 + 10 passed)
  - `cargo test -p nexus42 --lib` (648 passed)
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
# Diff basis (initial wave)
git log --oneline 068135ed..9d471bdc
# 9d471bdc merge(v1.44 P0): manuscript-audit preset + CLI entry
# ... (5 commits + merge)

cargo clippy -p nexus42 -p nexus-orchestration -- -D warnings   # clean
cargo test -p nexus-orchestration --test novel_manuscript_audit  # 14 passed
cargo test -p nexus42 --lib  # relevant AuditMode + resolve_* tests + others green
cargo test -p nexus42 --test review_master_cli  # related CLI surface green
```

---

## Revalidation

**Re-review type**: Targeted (QC re-review after fix wave per Assignment). Only the two blocking Warnings raised by this reviewer (F-QC2-001, F-QC2-002) plus the explicit in-scope surfaces listed in the dispatch (daemon-side world-binding enforcement R-V144P0-002, `validate_body_path` R-V144P0-004, typed `CliError::WorldRequiredForExtract` R-V144P0-008, preset split safety R-V144P0-001, volume-aware body_path filter R-V144P0-003, body_path resolve correctness). 

**Review range (fix wave only)**: `9d471bdc..44a12a6e`
- d6b9400e fix(v1.44): R-V144P0-001 split preset into review + extract, fix CLI dispatch
- 3297d925 fix(v1.44): R-V144P0-002..005,007,008 CLI handler hardening
- fc9f2f6d fix(v1.44): R-V144P0-006,009 plan verification fix + CLI integration tests
- 44a12a6e merge(v1.44 P0): fix wave — split preset + harden CLI for QC Request Changes

**Scope discipline**: P1 (review-master) and P2 changes that appear in the broader iteration branch diff are **out of scope** for this re-review and were not analyzed. Only P0 audit-chapter surfaces and the listed residual fixes were examined.

**Verification executed in this re-review session** (all clean; commands reproduced above):
- `cargo +nightly fmt --all --check`
- `cargo clippy --all -- -D warnings`
- `cargo test -p nexus-orchestration --test novel_manuscript_audit --test novel_manuscript_audit_review --test novel_manuscript_audit_extract` (14 + 7 + 10 passed)
- `cargo test -p nexus42 --lib` (648 passed)
- `git log --oneline 9d471bdc..44a12a6e` (exact range reproduced)

### Disposition of F-QC2-001 (World-bound extract precondition bypass)

**Status**: Resolved.

**Fix mapping**:
- Primary: d6b9400e — The unified preset was split. The new `novel-manuscript-audit-extract` preset declares:
  ```yaml
  world_binding:
    mode: required
  ```
  This is a daemon/preset gate (evaluated by the orchestration engine before schedule execution, consistent with `entity-scope-model.md` and the `world_binding` manifest contract). The CLI still performs an early check (now using the typed error below) before constructing the `AddScheduleRequest`.
- Supporting: 3297d925 — CLI early check preserved and upgraded to typed `CliError::WorldRequiredForExtract`. Accurate async messaging added. Runtime-lock invariant documented in the handler (R-V144P0-010).
- Test coverage: New dedicated `novel_manuscript_audit_extract.rs` (10 tests) including `extract_preset_requires_kb_extract_work`, `extract_sync_passes_world_id_and_work_id`, `extract_preset_has_no_review_state`. The original `worldless_work_returns_422_on_extract` (now in base test module) continues to assert the 422 path. The extract preset state machine contains only `load_chapter → extract_sync → done` with no review states.

**Security/correctness assessment**:
- A direct schedule creation for the extract preset on a worldless Work will now be rejected at the preset `world_binding: required` gate before any capability (including `kb.extract_work`) is invoked. The bypass surface described in the original finding is closed.
- Dispatch in the CLI is a compile-time constant string (`"novel-manuscript-audit-extract"`) selected by the `AuditMode` enum; no user-controlled preset id can be injected.
- The typed error path emits only the `work_id` (a public identifier) plus static suggestion text. No paths, secrets, or untrusted input are interpolated into the error message.
- No new attack surface introduced by the split: the two presets have disjoint, minimal state machines; they declare the same narrow capability set; legacy unified preset is retained only for backward compat with an explicit deprecation comment and is not the dispatch target for the new CLI.

### Disposition of F-QC2-002 (body_path verbatim copy, no normalization / traversal rejection)

**Status**: Resolved.

**Fix mapping**:
- 3297d925 introduces `validate_body_path` (called from the updated `resolve_audit_body_path`) with explicit rules:
  - Reject absolute paths (starts with `/`)
  - Reject path traversal (`..` anywhere)
  - Require `Works/` prefix (layout boundary)
  - Reject control characters (`char::is_control`)
- The function is exercised on the raw `body_path` value coming from the Work record before it is placed into the schedule `input.body_path` (and thus `source_locator` for `kb.extract_work` in the extract preset).
- Volume-aware lookup (R-V144P0-003) was added in the same change: the resolver now prefers `(chapter, volume)` match and falls back to chapter-only for legacy Works without a `volume` field. This improves correctness for multi-volume manuscripts without weakening the safety filter.
- Four new unit tests directly cover the rejection cases:
  - `validate_body_path_rejects_absolute`
  - `validate_body_path_rejects_traversal`
  - `validate_body_path_rejects_non_works_prefix`
  - `validate_body_path_rejects_control_chars`
  - Plus positive case `validate_body_path_accepts_valid_path` and integration via `resolve_audit_body_path_*` tests.

**Security/correctness assessment**:
- The original vector (malformed or attacker-influenced `body_path` from a Work record being passed verbatim into `kb.extract_work` source_locator and the load-chapter prompt) is now filtered at the CLI boundary before schedule creation.
- The filter is intentionally conservative and simple (string prefix + substring + char predicate). No complex parsing or regex that could itself be a source of issues.
- The `Works/` prefix + no-`..` check prevents escape to parent directories or sibling trees outside the per-Work layout. A path that legitimately points to another Work's chapter (data-integrity issue in the stored Work record) is still possible but is now bounded to the `Works/` subtree and cannot traverse outside the workspace or inject control characters. Such cross-Work reference would be an upstream manuscript-creation or data-corruption concern, not introduced or amplified by this on-demand audit path.
- The filtered value is what the extract preset receives in `source_locator`. The preset itself does not perform additional path validation (it is a capability argument), but the caller (CLI) now guarantees a safe value for the documented layout.
- Fail-fast for missing body_path (R-V144P0-007) was also added: if resolution yields `None`, the handler returns a clear `CliError::Config` before any schedule is created. This prevents downstream template-render failures from becoming confusing runtime errors.

### New surfaces from the fix wave (scoped review)

- **Preset split (d6b9400e)**: Two new embedded preset directories (`novel-manuscript-audit-review`, `novel-manuscript-audit-extract`) with their own `preset.yaml` and prompts. State machines are minimal and disjoint. No new capabilities beyond the declared set (`creator.inject_prompt`, `acp.prompt`, `kb.extract_work`). The extract preset correctly wires `world_id` and `body_path` (post-filter) into `kb.extract_work`. No shared mutable state or cross-preset leakage. Legacy preset kept in place with deprecation comment only. Assessed as low additional attack surface.
- **CLI dispatch change**: Simple `match` on the local `AuditMode` enum producing a constant preset id string. No user input flows into the preset selection. Safe.
- **Typed error (R-V144P0-008)**: `CliError::WorldRequiredForExtract { work_id }` with `Display` implementation containing only the work_id and static suggestion text. No interpolation of paths, user-controlled content, or secrets. The error is returned to the local CLI user only. No information leak.
- **Path validator + volume filter**: As analyzed above. Unit tests present and passing. No bypasses observed in the diff or by inspection of the predicate logic.
- No evidence of secret leakage, prompt injection amplification, or new privilege-escalation paths in the scoped changes.

### Residual / Suggestion notes (unchanged from wave 1)

The original Suggestions (F-QC2-003 template variable trust boundary, F-QC2-004 findings upsert visibility, F-QC2-005 generic daemon schedule validation for mode-dependent preconditions) were not in the blocking set and remain suggestions. They are appropriate for post-V1.44 or P-last follow-up and are not re-raised as Warnings for this targeted re-review.

### Updated verdict rationale

All required verification commands (fmt, clippy, the three P0-specific orchestration tests, full lib test suite) are clean. The two blocking Warnings (F-QC2-001, F-QC2-002) have been dispositioned as resolved with daemon-side gating (`world_binding: required`), explicit path validation (`validate_body_path` with tests), typed safe error handling, and fail-fast behavior. No new Critical or unresolved Warning findings were identified in the scoped P0 fix surfaces.

Per `mstar-review-qc` gate rules (Critical = 0 and unresolved Warning = 0 → Approve), the verdict for this reviewer is updated to **Approve**.

---

**Revalidation Evidence (reproduced in this session)**

```bash
# Exact fix-wave range (verbatim per Assignment)
git log --oneline 9d471bdc..44a12a6e
# 44a12a6e merge(v1.44 P0): fix wave — split preset + harden CLI for QC Request Changes
# fc9f2f6d fix(v1.44): R-V144P0-006,009 plan verification fix + CLI integration tests
# 3297d925 fix(v1.44): R-V144P0-002..005,007,008 CLI handler hardening
# d6b9400e fix(v1.44): R-V144P0-001 split preset into review + extract, fix CLI dispatch

cargo +nightly fmt --all --check
cargo clippy --all -- -D warnings
cargo test -p nexus-orchestration --test novel_manuscript_audit --test novel_manuscript_audit_review --test novel_manuscript_audit_extract
cargo test -p nexus42 --lib
```

(End of report)
