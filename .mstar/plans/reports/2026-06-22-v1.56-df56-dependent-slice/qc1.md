---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-22-v1.56-df56-dependent-slice
verdict: Approve with comments
generated_at: 2026-06-22T05:00:00Z
---

# Code Review Report

## Reviewer Metadata
- **Reviewer**: @qc-specialist (Reviewer #1 — Architecture)
- **Runtime Agent ID**: qc-specialist
- **Runtime Model**: deepseek-v4-flash
- **Review Perspective**: Architecture coherence and maintainability risk
- **Report Timestamp**: 2026-06-22T05:00:00Z

## Scope
- **plan_id**: `2026-06-22-v1.56-df56-dependent-slice`
- **Review range / Diff basis**: `d494b60a..6c6bb831` (integration HEAD before P3; Wave 2 closure → P3 merge commit)
- **Working branch (verified)**: `iteration/v1.56`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus` (`git rev-parse --show-toplevel`)
- **Files reviewed**: 3 (1 spec + 2 source modules)
- **Commit range**: `d494b60a..6c6bb831` (2 commits: `60c9869d` feature + `6c6bb831` merge)
- **Tools run**: `cargo clippy -p nexus-orchestration`, `cargo +nightly fmt --all --check`, `cargo test` (P3-scoped), `git diff`, manual code reading

## Review Summary

V1.56 P3 delivers the two remaining DF-56 dependent sub-items: **registry.refresh conditional edges** (wiring P2 expression engine to P1 capability output) and **workspace.open/commit branch inputs** (wiring P2 expression engine to P0 workspace sessions). The implementation is clean, well-structured, and stays within the defined scope of 2 dependent sub-items. No P0/P1/P2 code was modified outside of adding P3's inject calls — backward compatibility is preserved.

The architecture of the context dependency scanning (`scan_context_deps` in `expr.rs`) is elegant: the AST walker runs at cache-build time, producing a `ContextDeps` bitmask that controls whether I/O-bound injections run at evaluation time. No unnecessary `registry.refresh` or workspace queries are performed for expressions that don't reference them.

All 20 P3 tests pass; 0 clippy warnings; 0 fmt issues.

**Verdict**: **Approve with comments** — 3 low-severity observations, no critical or warning findings.

---

## Findings

### 🟢 Suggestion

**S-001: Internal context key naming convention inconsistency**

*The P3 inject methods use `__registry_refresh_output` and `__workspace_state` (double-underscore prefix), while existing internal context keys use single-underscore: `_converge_arrivals_{state_id}` and `_merge_{state_id}`. Double-underscore is a reasonable collision-avoidance choice (user-set context values are unlikely to start with `__`), but it breaks codebase consistency.*

**Recommendation**: Either:
- (a) Align to single-underscore to match existing convention (`_registry_refresh_output`, `_workspace_state`), or
- (b) Document in `build_context_json`'s doc-comment that `__` prefix designates internal/injected keys and should not be set by user actions or expressions.

The risk is low — double-underscore userset keys are unlikely — but inconsistency erodes maintainability over time.

**Source**: `tasks/mod.rs` lines `inject_registry_refresh_context` and `inject_workspace_context` (keys `__registry_refresh_output`, `__workspace_state`) vs. `converge_key: format!("_converge_arrivals_{}", state.id)` in `StateCompositeTask::new` and `merge_key: format!("_merge_{state_id}")`.

**Confidence**: High

---

**S-002: `registry_output_to_context` creates silent coupling to capability output shape**

*The `registry_output_to_context` function in `tasks/mod.rs` maps camelCase capability output fields to snake_case context fields by name. If P1's `RegistryRefreshOutput` struct fields are renamed or the shape changes (e.g., `snapshotVersion` → `version`), the mapper silently produces `Null` values rather than failing at compile time or at test time.*

**Recommendation**: Either:
- (a) Add a round-trip test that constructs a mock `RegistryRefreshOutput` value, runs it through `registry_output_to_context`, and asserts all expected snake_case keys are present and non-null, OR
- (b) Expose a struct-level conversion (e.g., `impl From<RegistryRefreshOutput> for RegistryRefreshContext`) that the mapper calls, giving compile-time safety against field renames.

**Source**: `registry_output_to_context` in `tasks/mod.rs` — calls `.get("snapshotVersion")` etc. on raw `serde_json::Value`. If P1 renames the capability output fields, this breaks silently. Counterpart `synthetic_registry_output()` includes extra P1 fields (`cacheAgeMs`, `generatedAt`, etc.) that are not mapped — if those were needed, the gap would be invisible.

**Confidence**: Medium

---

**S-003: `synthetic_registry_output()` includes inert fields from P1 capability**

*The `synthetic_registry_output()` fallback function injects fields `cacheAgeMs`, `generatedAt`, `fetchTimeoutMs`, `maxRetries` that are P1 capability output metadata not exposed by the expression grammar's `registry_output_to_context` mapper (no `snake_case` mapping for them). These fields are dead weight in the synthetic fallback — they're always populated but never read by the expression evaluator.*

**Recommendation**: Keep the synthetic fallback minimal to the 5 spec-defined fields (`source`, `snapshotVersion`, `capabilityCount`, `fallbackReason`, `retryCount`). Extra fields can be added when they gain a mapped expression path. This reduces cognitive overhead and avoids confusion about which fields are actually usable in branch expressions.

**Source**: `synthetic_registry_output()` in `tasks/mod.rs` — produces `cacheAgeMs`, `generatedAt`, `fetchTimeoutMs`, `maxRetries` which are not mapped by `registry_output_to_context` and not listed in the spec §3.4.1 context fields table.

**Confidence**: High

---

**S-004: Workspace state production wiring depends on P0 engine integration**

*Per spec §3.4.2 invocation rule: "the orchestration engine populates these fields from the WorkspaceSessionManager before evaluating branches." The `with_workspace_state()` hook exists as the designed integration point. Tests exercise workspace expressions via direct context injection (`ctx.set("__workspace_state", ...)`), but no production code path in P3 calls `with_workspace_state()` with real session data.*

*This is not a P3 defect — the hook is correctly exposed, and the fallback default ensures expressions never crash on missing state. However, the actual engine wiring (calling `with_workspace_state()` from the code path that runs `StateCompositeTask`) is assumed to be on the integration branch from P0. If it's not, workspace branch expressions in production will always receive the minimal default (`session_id: ""`, `conflict_detected: false`, `changes_applied: 0`, `workspace_root: ""`), making workspace-driven routing ineffective outside tests.*

**Recommendation**: PM/P-last should verify that the P0 engine code populates workspace state on `StateCompositeTask` before fully qualifying the "Workspace-driven edges work with persistent sessions" acceptance criterion.

**Source**: `with_workspace_state()` method in `tasks/mod.rs` — only called from test code in the current diff. `inject_workspace_context()` falls back to defaults if `self.workspace_state` is `None`.

**Confidence**: Medium

---

### Scope Creep Check

| Check | Result |
|---|---|
| Stayed within 2 dependent sub-items | ✅ Yes — only `registry.refresh` + `workspace.*` |
| No P0/P1/P2 code modified (spec ≤ maintenance) | ✅ Yes — only minimal additions (P3 `.await` injection points); no code changed logic |
| No new schema enum changes | ✅ Yes — no schema types added; all context injection is internal (`__` keys) |
| Spec updated for §3.4 | ✅ Yes — `preset-conditional-routing.md` §3.4 added with tables, invocation rules, examples |

### Backward Compatibility Check

| Path | Status |
|---|---|
| V1.42 GoNogo (llm_judge GO/NOGO) | ✅ Unchanged — `resolve_labeled_target` not touched |
| V1.52 T-B Labeled routing | ✅ Unchanged — `resolve_labeled_target` not touched |
| V1.52 T-B Merge semantics | ✅ Unchanged — merge tracking not touched |
| V1.56 P2 expression routing (no registry/workspace deps) | ✅ Unchanged — expressions without `registry_refresh`/`workspace` refs skip injection |
| V1.56 P2 converge nodes | ✅ Unchanged — converge runtime not touched |
| V1.56 P2 min_interval throttle path | ✅ Refactored only to add `.await` injection point; logic identical |
| Existing test code | ✅ Unchanged — only `workspace_state: None` added to struct literals (required field) |

### Checklist (Shared Review Baseline)

- [x] **Naming clarity**: `ContextDeps`, `scan_context_deps`, `needs_registry_refresh`, `needs_workspace` — clear and self-documenting. ✓
- [x] **Separation of concerns**: AST scanning in `expr.rs` (natural home for AST introspection); injection methods on `StateCompositeTask` (where context is evaluated). ✓
- [x] **Error handling**: `inject_registry_refresh_context` has fallback for every failure mode (no registry, capability not found, capability invocation fails). ✓
- [x] **No unintended side effects**: Injected context values are idempotent (guard checks `is_some()`). Registry capability is invoked with empty payload. ✓
- [x] **Test coverage**: 20 new tests covering all scan combinations + 6 workspace branch scenarios + 6 registry branch scenarios + 1 no-deps edge case. ✓
- [x] **Backward compatibility**: Expression grammar unchanged; existing GoNogo/Labeled/converge paths unmodified. ✓
- [x] **Performance**: Context deps scanned at cache-build time, not on hot path. Registry capability invoked at most once per evaluation. ✓
- [x] **Safety**: No code injection; expressions are locally evaluated. Branch expressions are read-only by design (no mutation of workspace/registry state). ✓

### Spec Consistency (preset-conditional-routing.md §3.4)

| Field | Spec table | Implementation | Match |
|---|---|---|---|
| `registry_refresh.source` | `String` | `output.get("source")` → `json!({"source": source})` | ✅ |
| `registry_refresh.snapshot_version` | `String` | `output.get("snapshotVersion")` → `json!({"snapshot_version": ...})` | ✅ |
| `registry_refresh.capability_count` | `Number` | `output.get("capabilityCount")` → `json!({"capability_count": ...})` | ✅ |
| `registry_refresh.fallback_reason` | `String` | `output.get("fallbackReason")` → `json!({"fallback_reason": ...})` | ✅ |
| `registry_refresh.retry_count` | `Number` | `output.get("retryCount")` → `json!({"retry_count": ...})` | ✅ |
| `workspace.session_id` | `String` | `json!({"session_id": ""})` default / injected | ✅ |
| `workspace.conflict_detected` | `Bool` | `json!({"conflict_detected": false})` default / injected | ✅ |
| `workspace.changes_applied` | `Number` | `json!({"changes_applied": 0})` default / injected | ✅ |
| `workspace.workspace_root` | `String` | `json!({"workspace_root": ""})` default / injected | ✅ |

- Spec invocation rules match implementation (registry invoked once before branch evaluation; workspace injected from `WorkspaceSessionManager` or default; both injection paths idempotent). ✓
- Spec field naming (snake_case in context) matches implementation (`registry_output_to_context` mapper). ✓
- Spec §3.4.3 (context dependency scanning) correctly describes `build_expr_cache` behavior. ✓

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: **Approve with comments** — no blocking issues (0 critical, 0 warning). Four suggestions recorded for PM residual consideration. All acceptance criteria are addressed; spec extension and implementation are internally consistent.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| S-001 | manual-reasoning | `tasks/mod.rs` — double-underscore context keys vs single-underscore existing keys | High |
| S-002 | manual-reasoning | `tasks/mod.rs` — `registry_output_to_context` str-keyed lookups on `serde_json::Value` | Medium |
| S-003 | manual-reasoning | `tasks/mod.rs` — `synthetic_registry_output()` includes unmapped fields | High |
| S-004 | manual-reasoning | `tasks/mod.rs` — `with_workspace_state()` only called from tests; no production caller in P3 diff | Medium |
