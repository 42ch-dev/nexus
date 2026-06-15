---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-15-v1.47-gate-remediation-audit"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-15

## Scope
- plan_id: 2026-06-15-v1.47-gate-remediation-audit
- Review range / Diff basis: `merge-base: 6acb5ae680c5c7f11050c82df6f0e4156c33f78e + tip: HEAD` (i.e. `git diff 6acb5ae680c5c7f11050c82df6f0e4156c33f78e..HEAD`)
- Working branch (verified): `feature/v1.47-gate-remediation-audit`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.47-p1-remediation`
- Files reviewed: 6
- Commit range (single implementation commit): `6acb5ae6..9a3ac5a9`
- Tools run: `git diff`, `git log`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings`, `cargo test -p nexus-orchestration --lib -- preset_gates`, `cargo test -p nexus-daemon-runtime --lib -- schedules`, `cargo test -p nexus42 --lib -- commands::creator::{works,run} errors`, `rg` sweep for residual `.mstar/knowledge/specs/` in user-facing strings.

## Architectural Context

This plan remediates two author-blocking residuals from V1.46:

- **R-V146P1-QC3-S1**: intake gate remediation cited the broken command `creator bootstrap --preset creative-brief-intake` — `--preset` overrides the **PRODUCTION** preset on `creator bootstrap`, not intake (intake is hardcoded to `creative-brief-intake` in `bootstrap.rs:303`). Per `creator-run-preset-entry.md` §3.2, intake is triggered only via `creator bootstrap`.
- **R-V146P1-QC3-S4**: runtime remediation strings embedded raw `.mstar/knowledge/specs/*.md` repo paths as the only user action — brittle and not user-facing.

The single implementation commit (`9a3ac5a9`) sweeps 6 files across 3 crates (`nexus-orchestration`, `nexus-daemon-runtime`, `nexus42`), replacing path strings with stable spec names and fixing the intake remediation command.

## Spec ↔ Code Alignment (Reviewer #1 focus)

Verified against normative sources:

| Claim in code | Spec source | Verdict |
|---|---|---|
| "intake is triggered only via `creator bootstrap`" | `creator-run-preset-entry.md` §3.2 line 78: *"intake is triggered only via `creator bootstrap`, not manual `creator run`."* | ✅ Aligned |
| `--preset creative-brief-intake` is wrong (`--preset` overrides PRODUCTION) | `bootstrap.rs` code: `--preset` sets production preset; intake hardcoded `creative-brief-intake` at line 303 | ✅ Accurate |
| intake remediation cites "novel-author-experience §3.2" | `novel-author-experience.md` §5 SSOT table: *"Missing scaffold / intake incomplete → Cite §3.2"* | ✅ Compliant |
| `creator bootstrap` creates a NEW Work (not fixing existing) | `bootstrap.rs` module doc line 4: *"Creates a new Work, optionally schedules an init preset, schedules intake"* | ✅ Acknowledged in code comment |

The developer's inline comment in `preset_gates.rs` (`R-V146P1-QC3-S1: ...`) correctly explains the architectural reasoning — this is exemplary traceability and prevents regression.

## Layered Boundary Assessment

The P1 sweep correctly distinguishes two layers:

| Layer | Path style | P1 action |
|---|---|---|
| **User-facing remediation / error copy** | Stable spec name (`"novel-author-experience §3.2"`, `"creator-run-preset-entry spec"`) | ✅ Swept (6 files) |
| **Developer-facing `//!` doc comments** | Repo-relative (`.mstar/knowledge/specs/orchestration-engine.md §4.2`) | ✅ Left unchanged (correct — developers have the repo) |

Verified via `rg -n "\.mstar/knowledge/specs/" crates/ --type rust -g '!**/tests/**'`: all 13 remaining references are `//!` module doc comments or test documentation — none in user-facing remediation strings. This is the right layer boundary.

## Acceptance Criteria Verification

| AC | Status | Evidence |
|---|---|---|
| **AC1**: R-V146P1-QC3-S1 repro fixed with test | ✅ Met | `intake_status_remediation_cites_executable_bootstrap` test in `preset_gates.rs` forces `intake_status: pending`, asserts remediation contains `creator bootstrap` and NOT `--preset creative-brief-intake`. Plus inline code comment documenting the closure. |
| **AC2**: No raw `.mstar/knowledge/specs/...` as only user action | ✅ Met | `rg` sweep confirms zero user-facing remediation strings embed raw paths. `no_gate_remediation_embeds_raw_dotmstar_paths` regression test iterates all gate helper branches. |
| **AC3**: Intake/scaffold gate failures cite executable `creator run` / `creator bootstrap` | ✅ Met | `intake_status` → `nexus42 creator bootstrap`; scaffold/filesystem → `creator bootstrap --init-preset novel-project-init`. Both executable. |
| **AC4**: No regression on V1.46 per-finding `routing_hint` | ✅ Met | `v146_routing_hint_behavior_unchanged` test verifies per-finding hints appear and no blanket `novel-chapter-review` footer. |

## Findings

### 🔴 Critical
*(none)*

### 🟡 Warning
*(none)*

### 🟢 Suggestion

- **S-1: No single source of truth for spec name / command string literals.** The strings `"novel-author-experience"`, `"creator-run-preset-entry spec"`, and `"nexus42 creator bootstrap --init-preset novel-project-init"` appear as hardcoded literals across 5–6 files (`preset_gates.rs`, `schedules.rs`, `run.rs`, `works/mod.rs`, `mod.rs`, `errors.rs`). If a spec is renamed or a command flag changes, all sites need manual updates. This is a **pre-existing pattern from V1.46** (P1 maintains it consistently, does not make it worse). Future plan could extract `const SPEC_NOVEL_AUTHOR_EXPERIENCE: &str` and a `bootstrap_init_cmd()` helper. Not blocking — P1's scope is the broken-command/path sweep, not refactoring string constants.

- **S-2: `intake_status` remediation operationally ambiguous for existing-Work case.** The message *"Intake (`creative-brief-intake`) has not completed on this Work. Intake runs automatically during `nexus42 creator bootstrap`."* is technically honest but does not clarify that `creator bootstrap` creates a **NEW** Work — it does not complete intake on the failed Work. A user with an existing Work stuck at `intake_status: pending` has no CLI command to re-trigger intake on that Work (must use `--force-gates --reason` or abandon the Work). The developer's code comment (`preset_gates.rs` R-V146P1-QC3-S1 block) acknowledges this honestly. The plan's ACs are met (executable command cited, no `.mstar/` paths), and the fix is a net improvement over V1.46's actively-broken `--preset creative-brief-intake`. Recommend tracking the deeper "intake re-trigger on existing Work" UX gap as a residual for a future plan. Not blocking.

- **S-3: Test seam smell in `no_gate_remediation_embeds_raw_dotmstar_paths`.** The `intake_status` branch requires a separate "force-collect" block because `make_work()` defaults to `intake_status: complete` (the gate passes without producing remediation). A fixture variant like `make_work_with_pending_intake()` or parameterizing the loop would be cleaner. The test is well-commented and functionally correct — the smell is structural, not a correctness issue.

## Positive Observations

- The inline `R-V146P1-QC3-S1` code comment in `preset_gates.rs` is exemplary: it explains **why** the old command was wrong (`--preset` overrides PRODUCTION, `creator bootstrap` creates a new Work), cites the spec section, and justifies the chosen remediation. This level of traceability prevents regression and aids future reviewers.
- The `no_gate_remediation_embeds_raw_dotmstar_paths` test iterates **all** `work_field_remediation` branches + filesystem + previous_preset helpers — strong regression coverage for the path-string invariant.
- Stacked version comments (`// V1.47 P1: ...` over `// V1.46 P1 (spec hygiene): ...`) preserve audit trail without rewriting history. Acceptable traceability convention.
- Pre-existing `master_decision_timeout::repeated_sweeps_remain_stable` flake and baseline clippy items were **not** introduced by P1 (confirmed: clippy gate `-D warnings` passes clean on all 4 affected crates).

## Source Trace

- Finding ID: S-1
- Source Type: manual-reasoning
- Source Reference: `rg -n "novel-author-experience|creator-run-preset-entry spec" crates/ --type rust` (5+ files)
- Confidence: High

- Finding ID: S-2
- Source Type: manual-reasoning + spec-alignment
- Source Reference: `preset_gates.rs:362-372` (intake_status remediation + R-V146P1-QC3-S1 comment); `bootstrap.rs:4` module doc; `creator-run-preset-entry.md` §3.2 line 78
- Confidence: High

- Finding ID: S-3
- Source Type: manual-reasoning
- Source Reference: `preset_gates.rs` test `no_gate_remediation_embeds_raw_dotmstar_paths` (force-collect block ~line 1175-1195)
- Confidence: Medium

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

All four acceptance criteria are met with verifiable evidence (tests + spec citations). No Critical or Warning findings. Three Suggestions are pre-existing patterns or out-of-scope architectural observations, none of which block merge. The implementation correctly fixes the broken `--preset creative-brief-intake` command (R-V146P1-QC3-S1), sweeps all user-facing `.mstar/` path strings to stable spec names (R-V146P1-QC3-S4), preserves V1.46 per-finding `routing_hint` behavior (AC4), and adds strong regression guards. fmt clean, clippy clean (`-D warnings`), all affected tests pass.
