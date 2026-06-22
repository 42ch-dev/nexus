---
plan_id: 2026-06-22-v1.58-df44-reference-refresh-pipeline
reviewer: qc-specialist
reviewer_index: 1
focus: architecture-maintainability
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
working_branch: iteration/v1.58
diff_basis: d443e855..af82ad39
reviewed_at: 2026-06-22T...
verdict: Approve
report_kind: qc
generated_at: 2026-06-22T...
---

# QC1 — V1.58 P1 DF-44 Reference Refresh — Architecture/Maintainability Review

## Reviewer Metadata
- **Reviewer**: @qc-specialist (Reviewer #1)
- **Runtime Agent ID**: qc-specialist
- **Runtime Model**: deepseek/deepseek-v4-flash
- **Review Perspective**: Architecture coherence and maintainability risk — capability placement, DB schema, scheduler lifecycle, spec cross-references, P0-P1 cross-coupling
- **Report Timestamp**: 2026-06-22

## Scope
- **plan_id**: `2026-06-22-v1.58-df44-reference-refresh-pipeline`
- **Review range / Diff basis**: `d443e855..af82ad39`
- **Working branch (verified)**: `iteration/v1.58` (HEAD `af82ad39` — integration branch with P0 + P1 merged)
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: 12 (source, migration, specs, tests, plan)
- **Commit range**: `d443e855..af82ad39` (P0 + P1 + sqlx restoration on integration branch)

---

## Summary

The P1 implementation is architecturally sound. The `nexus.reference.refresh` capability is correctly placed in the orchestration `CapabilityRegistry` (not `host_tool_registry()`), the DB schema extension is clean and forward-compatible, the refresh-scheduler lifecycle correctly mirrors established patterns (`stale_findings_watcher`, `cron_supervisor`, `auto_chronology`), and the Draft spec `reference-knowledge.md` covers all 7 required sections with valid cross-references.

**1 Medium** finding (metrics implementation gap vs. documented promise) and **2 Low** findings (shared client opportunity, error message persistence). No High findings. **Verdict: Approve**.

---

## Findings

### Medium severity

#### M-1: Metrics implementation does not match doc comment promise

**Location**: `crates/nexus-daemon-runtime/src/refresh_scheduler.rs` line 17 (doc comment) vs. lines 172-209 (implementation)

**Description**: The module-level doc comment says:
```rust
//! - tracing spans at each refresh attempt; metrics counters for
//!   total/success/failure.
```
But the implementation in `run_one_refresh_tick()` only has local `u64` variables (`success_count`, `failure_count`) that are logged via `tracing::info!`. These are ephemeral — not queryable counters. By contrast, P0's `registry.refresh` uses `AtomicU64` counters (`refresh_total`, `refresh_success`, `refresh_failure`, `refresh_cache_hit`) with public reader methods, per V1.57 residual R-V156P1-L007.

The `reference-knowledge.md` §3 spec correctly says "success/failure counters per tick" (tick-level logging), so the spec is satisfied. The doc comment over-promises relative to the implementation.

**Impact**: Low in practice — tracing logs do provide observability. But the inconsistency with P0's structured metrics pattern means the refresh scheduler has no programmatic metrics surface. If a future plan wants to expose scheduler health via monitoring endpoints, this gap requires remediation.

**Recommendation**: Either:
1. Upgrade local vars to `AtomicU64` counters matching P0's pattern (recommended for consistency), or
2. Replace the doc line with "tracing counters" to match actual behavior.

### Low severity

#### L-1: `stale_threshold_seconds` injected into dynamic SQL via `format!()`

**Location**: `crates/nexus-local-db/src/reference_source.rs` lines 455-488 (`find_stale_sources`)

**Description**: The query uses `format!()` to inject `stale_threshold_seconds` and `limit` directly into the SQL string:
```rust
let rows = sqlx::query(&format!(
    "... datetime('now', '-{stale_threshold_seconds} seconds') ... LIMIT {limit}",
))
```
While this is acknowledged with a `// SAFETY: dynamic SQL` comment, SQLite `datetime()` arithmetic means this is a string concatenation, not a parameter binding. The `stale_threshold_seconds` comes from an env var (`NEXUS_DAEMON_REFRESH_SCHEDULER_STALE_THRESHOLD_SECS`) parsed as `i64` and validated > 0, so the practical risk is near-zero (local daemon env var, validated before use). Still, it's inconsistent with the project's compile-time sqlx macro preference and the other DAO functions in this same file which use `sqlx::query!()` with parameters.

**Impact**: Minimal. Local-only daemon env var, validated > 0. Not exploitable in practice.

**Recommendation**: Accept (waived by the `// SAFETY` comment) or refactor when sqlx adds parameterized datetime arithmetic support.

#### L-2: Separate `LazyLock<reqwest::Client>` in P0 and P1 — missed reuse

**Location**: `crates/nexus-orchestration/src/capability/builtins/registry.rs` line 71 (`SHARED_CDN_CLIENT`) vs. `crates/nexus-orchestration/src/capability/builtins/reference_refresh.rs` line 29 (`HTTP_CLIENT`)

**Description**: Both P0's `registry.refresh` and P1's `nexus.reference.refresh` define separate `LazyLock<reqwest::Client>` statics with different configurations:
- P0: `redirect(Policy::limited(0))` + connection pooling
- P1: 30s timeout + custom user-agent `"nexus42/... reference-refresh"`

These could share a common client factory. The different configurations may justify separation (P0 prohibits redirects for security; P1 allows default redirects for fetching reference URLs), so sharing is not strictly better. However, the connection pool duplication is worth noting.

**Impact**: Minimal. Two connection pools instead of one is not a resource concern for a local daemon.

**Recommendation**: Consider a shared `nexus-orchestration` HTTP client utility if a third capability needs HTTP fetching. No action needed for V1.58.

#### L-3: `mark_refresh_error` does not persist error message

**Location**: `crates/nexus-local-db/src/reference_source.rs` lines 421-437, called from `reference_refresh.rs` lines 148-153, 172-177, 255-259

**Description**: The `mark_refresh_error` function takes `_error_msg: &str` (prefixed with `_` — unused) and only sets `refresh_status = 'error'` + updates `updated_at`. The actual error message is logged via `tracing::warn!` in the caller but never stored in the DB. This means a user/CLI querying `reference_sources` can see that a source is in error state but cannot determine _why_.

The DB schema has no `last_error_message` column — this is by design per the comment at line 427-428. The comment says the error is "logged via tracing," but tracing logs are ephemeral and not queryable through the product surface.

**Impact**: Low — the error status is correctly set. P3 CLI work may need to add error message persistence if the `nexus42 reference refresh` subcommand surfaces error details.

**Recommendation**: Defer to P3. If P3's CLI needs to display error reasons, add a `last_error_message TEXT` column and populate it.

---

## Architecture Properties Verified

### 1. Capability placement: orchestration `CapabilityRegistry`, not `host_tool_registry()`

**✅ Correct** — `nexus.reference.refresh` is registered in the orchestration `CapabilityRegistry`, consistent with the pattern:
- `game_bible.section_status.update` (V1.56 P-last) also uses orchestration registry (per `acp-capability-set.md` §4.3)
- The refresh scheduler dispatches internally, not through ACP
- `capability-registry.md` §2.8 explicitly states: *"Not registered in `host_tool_registry()` (reference-source-scoped, not ACP-facing)"*
- `acp-capability-set.md` §4 roster row 147 marks `registry row ref: orchestration`

### 2. DB schema forward-compatibility

**✅ Clean design** — Verified:
- Three additive `ALTER TABLE ADD COLUMN` statements — zero migration risk for existing rows (all nullable or have DEFAULT)
- Partial index `idx_reference_sources_refresh_policy WHERE refresh_policy != 'offline'` correctly serves the scheduler query
- `idx_reference_sources_refresh_status` enables fast filtering
- Policy model is string-based (`TEXT`), not a native SQL enum — forward-compatible with new policy values
- Default `'offline'` for `refresh_policy` preserves existing behavior for all registered sources
- `updated_at` column already existed in the initial migration — correctly referenced by DAO functions

### 3. Refresh scheduler lifecycle

**✅ Correct lifecycle** — Verified:
- `spawn_refresh_scheduler` is a `tokio::spawn` task with `JoinHandle` (same pattern as `stale_findings_watcher`, `cron_supervisor`, `auto_chronology`)
- 60s initial delay before first cycle (avoids blocking daemon boot)
- `tokio::select!` with `shutdown_notify` for graceful shutdown — mirrors all sibling subsystems
- `MissedTickBehavior::Delay` correctly handles laptop sleep / long pauses
- `run_one_refresh_tick` is `pub` for hermetic integration tests without spawning the loop
- Configurable interval (default 3600s) + stale threshold (default 86400s) via env vars
- Integration in `boot.rs` Section 4e (lines 547-563) follows the exact same pattern as Sections 4b-4d
- ⚠️ Metrics implementation gap (see M-1 above)

### 4. Spec cross-reference validity

**✅ All valid** — Verified:
- `reference-knowledge.md` has all **7 sections** (0-7): Document position, Scope, Refresh policy model, Refresh scheduler contract, DB schema, Capability IDs and admission, Integration points, Examples
- Cross-references in Coordinates line: `acp-capability-set.md` §4 ✅, `capability-registry.md` §2.8 ✅, `daemon-runtime.md` §4e ✅, `entity-scope-model.md` ✅
- Section 5 correctly documents deferred sibling capabilities (`nexus.reference.refresh_policy.get`, `nexus.reference.refresh_status`)
- Section 6 Integration points correctly lists all 6 integration files
- Section 7 Examples include success and policy_blocked outputs — match the handler output schema
- Draft header: `Status: Draft (V1.58 P1)`, `Document class: Draft overlay`, `Promotion: Master at V1.58 P-last` — format correct per `specs/AGENTS.md`

### 5. Cross-validation test updates

**✅ Correctly updated** — Verified:
- `crates/nexus-orchestration/tests/capability_registry.rs`: `registry_has_twenty_six_builtins` — counter 25→26 ✅
- `crates/nexus-orchestration/src/capability/mod.rs`: `registry_has_twenty_six_builtins` — counter 25→26 ✅ (+ `registry_iter_returns_all` at 26 ✅)
- `crates/nexus-daemon-runtime/src/capability_registry.rs`: `catalog_registry_invariant_all_ids_present` — this tests `host_tool_registry()` against the acp catalog, not orchestration capabilities. `nexus.reference.refresh` is correctly NOT in `host_tool_registry()`, so this test does not need updating for P1. The test correctly continues to pass because the catalog row for `nexus.reference.refresh` uses `registry row ref: orchestration` (not `host_tool`).

### 6. T3/T10 skipped decisions (sibling capabilities and codegen)

**✅ Rationale sound** — Verified:
- **T3 deferred**: `nexus.reference.refresh_policy.get` and `nexus.reference.refresh_status` deferred to P3. Rationale is documented in `reference-knowledge.md` §5: *"P1 ships only `nexus.reference.refresh` as the core pipeline capability."* A single capability is sufficient for the scheduler-driven pipeline (which dispatches refresh by source ID, doesn't need policy/status readers). Policy changes are already possible via the existing `set_refresh_policy` DAO.
- **T10 skipped**: No `schemas/` changes because `nexus.reference.refresh` is orchestration-internal, not ACP-facing. No new wire types cross the ACP boundary (the refresh-scheduler dispatches internally via `CapabilityRegistry`). This is architecturally correct — adding schema contracts would be premature for a non-ACP-facing capability.

### 7. P0-P1 cross-coupling

**✅ No conflict** — Verified:
- **`LazyLock<reqwest::Client>`**: P0 has `SHARED_CDN_CLIENT` (registry.rs), P1 has `HTTP_CLIENT` (reference_refresh.rs). Different statics, different files, different configurations. No conflict. Minor reuse opportunity documented as L-2.
- **Metrics counters**: P0 uses `AtomicU64` (persistent, queryable); P1 uses local `u64` (ephemeral, logged). Different conventions but no conflict. Gap documented as M-1.
- **Naming**: Different metric naming conventions exist, but there's no name collision since P0's counters are in the registry module and P1's are local to `run_one_refresh_tick`.
- **DB access**: Both P0 and P1 use `SqlitePool` from the workspace state; no cross-coupling concerns.

### 8. Spec promotion path

**✅ Correct** — Verified:
- `reference-knowledge.md` header correctly declares: `**Promotion**: Master at V1.58 P-last`
- `Document class: Draft overlay` — correct for iteration-scoped revision per `specs/AGENTS.md`
- Draft has 7 sections with all required content
- `daemon-runtime.md` §4e correctly referenced in multiple places (reference-knowledge §6, boot.rs Section 4e comment)
- The Draft overlay pattern matches precedent (e.g., `capability-registry.md` V1.57 P0/P1 overlay)

---

## Verdict Reasoning

Approve with 0 High, 1 Medium, 3 Low findings.

**Approve rationale**:
1. The core architecture decision (orchestration `CapabilityRegistry` vs `host_tool_registry()`) is correct and consistent with existing patterns (`game_bible.section_status.update` precedent).
2. DB schema is clean, additive, and forward-compatible — no migration risk.
3. Scheduler lifecycle mirrors the three established daemon subsystems identically in pattern.
4. All spec cross-references are valid; Draft spec is complete.
5. No P0-P1 conflicts detected — both sets of changes coexist safely.
6. T3/T10 deferred/skipped with documented rationale — no oversight.

**M-1** (metrics doc/impl mismatch) does not block approval because:
- The spec (`reference-knowledge.md` §3) requires "counters per tick" which the `tracing::info!` logging satisfies
- The over-promise is in the module doc comment only, not in the spec
- Ephemeral log-based counters are functional for debugging
- A targeted fix can upgrade to `AtomicU64` counters without architectural change

**Findings to track as residuals**: M-1, L-2, L-3 (L-1 is accepted as `// SAFETY` waiver).

---

## Cross-Plan Concerns

### P0 (Workspace OCC Hardening) — No cross-coupling issues
Both `SHARED_CDN_CLIENT` (P0) and `HTTP_CLIENT` (P1) coexist cleanly in different capability modules. The P0 structured metrics (`AtomicU64`) could serve as a pattern for upgrading P1's metrics (see M-1).

### P2 (Capability Quality Convergence) — Minor overlap
P2's scope includes `reference-knowledge.md` spec hygiene if the quality convergence plan touches the reference-knowledge Draft. No breaking changes expected since P1's Draft is additive.

### P3 (Reference CLI and Cross-Cut Tests) — Dependency noted
- P3 will need the error message persistence tracked in L-3 if the CLI surfaces error details
- P3 will implement body file I/O (currently stubbed as `let _ = content_path;` in `reference_refresh.rs` lines 206-207)
- Sibling capabilities (`refresh_policy.get`, `refresh_status`) deferred to P3 — the `CapabilityRegistry` has capacity to register them
- The `find_stale_sources` query's dynamic SQL (L-1) may need hardening if P3 adds the CLI as an additional consumer

### P-last (Spec Promotion) — Path clear
`reference-knowledge.md` is correctly marked as Draft with promotion to Master at P-last. No content conflicts expected — the Draft is self-consistent and cross-references are already in place. The `daemon-runtime.md` §4e amendment is minimal and self-contained.

---

## Source Trace
| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|-----------------|------------|
| M-1 | manual-reasoning | `refresh_scheduler.rs` doc vs. impl | High |
| L-1 | manual-reasoning | `reference_source.rs:455-488` format! injection | High |
| L-2 | manual-reasoning | `registry.rs:71` vs `reference_refresh.rs:29` | High |
| L-3 | manual-reasoning | `reference_source.rs:421-437` unused error param | High |

## Summary
| Severity | Count |
|----------|-------|
| 🔴 High | 0 |
| 🟡 Warning | 0 |
| 🔵 Medium | 1 |
| 🟢 Low | 3 |

**Verdict**: **Approve** — architecture is coherent, maintainability risks are documented and minor.
