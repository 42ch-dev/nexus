---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-10-v1.40-hygiene"
verdict: "Request Changes"
generated_at: "2026-06-10"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: architecture coherence and maintainability risk
- Report Timestamp: 2026-06-10T23:00:00Z

## Scope
- plan_id: 2026-06-10-v1.40-hygiene
- Review range / Diff basis: iteration/v1.40..feature/v1.40-hygiene (cece6439..76a5461d)
- Working branch (verified): feature/v1.40-hygiene
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 10
- Commit range: cece6439..76a5461d (6 commits)
- Tools run: cargo clippy --all -- -D warnings, cargo test -p nexus-orchestration -p nexus42 -p nexus-local-db -p nexus-daemon-runtime

## Findings
### 🔴 Critical
- **C-1: Supervisor `tick_inner` WHERE filter breaks schedule dependency resolution** → `crates/nexus-orchestration/src/schedule/supervisor.rs:161-167` and `:817-823`
  The `tick_inner` and `resume_running` methods now filter `WHERE status IN ('pending', 'running', 'paused')`, excluding completed/cancelled schedules from the SELECT. However, both methods build a `completed_ids` set from the query results (lines 186-188, 838-840) and pass it to `admit()` for dependency checking. Since completed/cancelled rows are now excluded, `completed_ids` is **always empty**, and any schedule with `depends_on` entries will be **permanently blocked** from admission — the dependency gate in `admission.rs:159-163` will never find a satisfied dependency.
  
  Impact: `on_schedule_terminal` transitions a schedule to Completed, then calls `self.tick()` → `tick_inner()`, but `tick_inner` no longer sees the just-completed schedule. Schedules that depend on it remain stuck in Pending forever.
  
  → Fix: either (a) add `'completed', 'cancelled', 'failed'` back to the WHERE clause (accepting the O(N) scan tradeoff for correctness), or (b) split the query: load actionable schedules with the scoped WHERE, and separately load completed/cancelled IDs for dependency resolution only (e.g., `SELECT schedule_id FROM creator_schedules WHERE status IN ('completed','cancelled')`).

- **C-2: 16 test compilation failures — missing `auto_chain_interrupted` field in `PatchWorkRequest`** → `crates/nexus-daemon-runtime/tests/works_api.rs:330,358,530,552,597,627,816,873,902,953,977,1042,1114,1159,1200` and `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1332`
  The new `auto_chain_interrupted: Option<bool>` field was added to `PatchWorkRequest` (works.rs:185) but **16 test sites** that construct `PatchWorkRequest` were not updated. This causes `error[E0063]: missing field 'auto_chain_interrupted'` in the `nexus-daemon-runtime` test suite.
  
  → Fix: add `auto_chain_interrupted: None,` to all 16 `PatchWorkRequest { ... }` construction sites in `works_api.rs` and `works.rs:1332`.

### 🟡 Warning
- **W-1: Unused `FromRow` import in findings.rs test module** → `crates/nexus-local-db/src/findings.rs:593`
  `use sqlx::{FromRow, SqlitePool};` imports `FromRow` which is not used in the `#[cfg(test)] mod tests` block. The `SeverityCountRow` struct that derives `FromRow` is defined in the parent module, not in tests. Generates `warning: unused import: 'FromRow'` during `cargo test -p nexus-local-db`.
  
  → Fix: change to `use sqlx::SqlitePool;` (remove `FromRow` from the import).

- **W-2: `preset_version_for_id` hardcoded mapping is fragile** → `crates/nexus-orchestration/src/auto_chain.rs:419-426`
  The `preset_version_for_id()` function maps preset IDs to versions with a hardcoded `match` statement. The comment correctly notes "Must be kept in sync with embedded-presets/*/preset.yaml version: field." However, there is no compile-time or test-time enforcement of this sync. If a developer bumps a version in `preset.yaml` but forgets to update this mapping, the stored `preset_version` in `creator_schedules` will be stale, potentially causing the loader to select wrong template versions.
  
  → Fix: add a unit test that reads each embedded `preset.yaml`, extracts the `version` field, and asserts it matches `preset_version_for_id(preset_id)`. This would catch drift at `cargo test` time.

### 🟢 Suggestion
- **S-1: `ACH_COUNTER` mask comment is misleading** → `crates/nexus-orchestration/src/auto_chain.rs:362`
  `counter & 0x00FF_FFFF` masks to 24 bits (~16.7M unique values). The comment says "per-process monotonic counter for collision resistance" but the mask means the counter wraps after ~16.7M increments per process lifetime. For a long-running daemon, this could theoretically collide (though practically unlikely). Consider documenting the wrap behavior explicitly or using the full 32-bit range.

- **S-2: Duplicate SQL query string in `tick_inner` and `resume_running`** → `crates/nexus-orchestration/src/schedule/supervisor.rs:161-167, :817-823`
  The same `SELECT ... FROM creator_schedules WHERE status IN (...)` query string appears verbatim in two methods. If the WHERE clause changes (e.g., to fix C-1), both sites must be updated. Consider extracting to a `const` or helper function.

- **S-3: `SeverityCountRow` could be `pub(crate)` for clarity** → `crates/nexus-local-db/src/findings.rs:370`
  The struct is currently private (no visibility modifier), which is correct since it's only used in `count_open_findings_by_severity`. However, `sqlx::FromRow` derive on a private struct is slightly unusual — consider adding `pub(crate)` to signal intent that it's an internal row type, not an oversight.

## Source Trace
- Finding ID: C-1
- Source Type: manual-reasoning (code flow analysis)
- Source Reference: git diff cece6439..76a5461d — crates/nexus-orchestration/src/schedule/supervisor.rs
- Confidence: High

- Finding ID: C-2
- Source Type: cargo-test (compilation failure)
- Source Reference: `cargo test -p nexus-daemon-runtime` → 16× E0063
- Confidence: High

- Finding ID: W-1
- Source Type: cargo-test (compiler warning)
- Source Reference: `cargo test -p nexus-local-db` → `warning: unused import: 'FromRow'`
- Confidence: High

- Finding ID: W-2
- Source Type: manual-reasoning (maintainability analysis)
- Source Reference: crates/nexus-orchestration/src/auto_chain.rs:419-426 vs embedded-presets/*/preset.yaml
- Confidence: Medium

- Finding ID: S-1
- Source Type: manual-reasoning (code review)
- Source Reference: crates/nexus-orchestration/src/auto_chain.rs:22,362
- Confidence: Low

- Finding ID: S-2
- Source Type: manual-reasoning (DRY analysis)
- Source Reference: crates/nexus-orchestration/src/schedule/supervisor.rs:161-167, :817-823
- Confidence: Medium

- Finding ID: S-3
- Source Type: manual-reasoning (style review)
- Source Reference: crates/nexus-local-db/src/findings.rs:370
- Confidence: Low

## Checklist Results

### Architecture coherence and maintainability risk

- [x] **ULID suffix change preserves schedule ID format contract** — `ACH{timestamp}{:06x}` format adds 6 hex digits. No downstream consumers parse the ACH prefix format structurally; the ID is treated as an opaque string. No breakage.
- [x] **ID mint SSOT correctly eliminates duplication** — `mint_finding_id()` is now the single source; handler and `create_finding_from_review` both use it. No remaining inline `format!("fnd_{}", ...)` calls.
- [x] **CHECK constraint migration works alongside existing rows** — `ALTER TABLE findings ADD CONSTRAINT` is additive; existing rows with valid enum values pass. Runtime `validate_finding_enums()` provides a second guard. Good defense-in-depth.
- [ ] **Supervisor scoped `tick_inner` SELECT preserves edge cases** — ❌ **FAILS**: the WHERE filter breaks dependency resolution (see C-1).
- [x] **Waived UX residuals have adequate documentation** — N1-N3, W-5, S3 each have closure_notes referencing specific commits and rationale. Documentation is in preset.yaml headers, handler doc comments, and status.json closure fields.
- [x] **Closure fields on residuals are consistent** — All resolved/waived entries have `closure_note`, updated `decision`, and `target`. Lifecycle transitions are correct.
- [x] **Preset versioning policy comment correctly documents convention** — preset.yaml header and `preset_version_for_id` doc comment describe breaking vs non-breaking change rules.
- [ ] **Maintainability smells** — W-1 (unused import), W-2 (fragile hardcoded mapping), S-2 (duplicate SQL).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

**Rationale**: C-1 is a behavioral regression that breaks schedule dependency resolution — any schedule with `depends_on` entries will be permanently blocked from admission. C-2 causes 16 test compilation failures. Both must be resolved before approval.
