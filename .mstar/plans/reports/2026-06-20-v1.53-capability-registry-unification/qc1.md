---
plan_id: 2026-06-20-v1.53-capability-registry-unification
working_branch: feature/v1.53-capability-registry-unification
review_cwd: main worktree
review_range: 71dc6b1d..69594902
reviewer_index: 1
focus: architecture/maintainability
date: 2026-06-20
verdict: Request Changes
---

# QC #1 Review — V1.53 P0 CapabilityRegistry Unification (architecture/maintainability)

## Summary

Reviewed the assigned range `71dc6b1d..69594902` on `feature/v1.53-capability-registry-unification`, covering the registry implementation, host executor cutover, P0 plan roadmap, and the `capability-registry.md` Draft overlay. The checkout aligned to `/Users/bibi/workspace/organizations/42ch/nexus`, branch `feature/v1.53-capability-registry-unification`, tip `69594902`. The requested completion report path was absent, so this review used the assignment handoff plus plan/spec source files as the implementation report authority.

Architecturally, the migration history is mostly clean: the three implementation sub-phases are visible as single commits (`1d8b4452`, `85559d0d`, `e8a39db4`), followed by style and documentation commits; the old `HostToolExecutor` match dispatch table is removed; and the bridge/catalog authority chain is documented without reviving skills-export. The `RegistryHandlerFn` HRTB is appropriate for borrowing request/state/creator into boxed async handlers, and the row type is easy to extend for P1.

However, two maintainability issues block approval against P0 acceptance. First, registry metadata is not yet the runtime SSOT for id/admission because `TOOL_ALLOWLIST` remains a separate required runtime list. Second, the handler test-vector field is not mechanically validated and already contains one stale test name. Both issues directly affect P1’s ability to add five tools additively without drift.

## Verification evidence

- `git checkout feature/v1.53-capability-registry-unification` → already on branch.
- `git rev-parse --show-toplevel && git branch --show-current && git rev-parse --short HEAD` → `/Users/bibi/workspace/organizations/42ch/nexus`, `feature/v1.53-capability-registry-unification`, `69594902`.
- `git log --oneline 71dc6b1d..69594902` → six commits: introduce, cutover, cleanup, clippy, fmt, docs/plan.
- `git diff --stat 71dc6b1d..69594902` → 5 files, 1031 insertions, 79 deletions.
- `grep -rn 'fn dispatch_tool\|match.*"nexus\.\|fn execute_old' crates/nexus-daemon-runtime/src/ --include="*.rs"` → only `DaemonToolDispatch` trait impl plus policy wildcard check; no old `nexus.*` match dispatch table found.
- `grep -n 'pub struct CapabilityRow\|pub enum\|impl CapabilityRegistry' ... | head -20` → expected registry types present.
- `grep -A 3 '^### ' .mstar/knowledge/specs/capability-registry.md | head -50` → §2.1–§2.7 field semantics present.
- `cargo check -p nexus-daemon-runtime` → passed.
- `cargo clippy -p nexus-daemon-runtime -- -D warnings` → passed.
- `cargo test -p nexus-daemon-runtime --lib capability_registry` → passed, 7 tests.
- Extra check: `grep` for all declared `handler_test_vector.test_fn_name` values found 7 of 8 actual functions; `schedule_status_returns_ids` is missing.

## Findings

### Blocking / High severity
(none)

### Medium severity
- R-V153P0QC1-001: Registry is not the runtime SSOT for supported IDs/admission
  - Severity: medium
  - Scope: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:45-50`, `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:171-173`, `crates/nexus-daemon-runtime/src/capability_registry.rs:260-449`
  - Decision: fix
  - Evidence: `TOOL_ALLOWLIST` is still declared separately as “all V1.34 tool IDs” and “will remain as the runtime allowlist”; admission rejects unknown tools with `if !TOOL_ALLOWLIST.contains(&req.tool_name.as_str())`, before registry lookup. Separately, the registry contains the row IDs and admission vectors.
  - Note: P0 acceptance says the registry owns id/access/admission and P1 should add tools additively. With this shape, P1 must update both `TOOL_ALLOWLIST` and `host_tool_registry()`, so id/admission can drift. Prefer deriving the allowlist/admission decision from `CapabilityRegistry` rows, or add a strict cross-validation test that fails on any mismatch.

- R-V153P0QC1-002: Handler test-vector field is not enforced and already stale
  - Severity: medium
  - Scope: `crates/nexus-daemon-runtime/src/capability_registry.rs:372-375`, `crates/nexus-daemon-runtime/src/capability_registry.rs:491-510`, `.mstar/knowledge/specs/capability-registry.md:184-187`
  - Decision: fix
  - Evidence: the schedule row declares `test_fn_name: "schedule_status_returns_ids"`, but searching the crate finds no matching test function. The current registry test only checks that `test_fn_name` is non-empty, while the spec says every `TestVector::test_fn_name` must correspond to an actual test function.
  - Note: Either rename the vector to an existing schedule-status test or add the missing test. More importantly, add a maintainable enforcement mechanism (even a static accepted-name set in unit tests for now) so P1 rows cannot carry decorative, stale vectors.

### Low severity
(none)

### Nit / observation
- The HRTB on `RegistryHandlerFn` is idiomatic for the borrowed async wrapper pattern, but P1 maintainers would benefit from one tiny example near the type alias showing why `for<'a>` is required.
- `capability-registry.md` draws a sharp catalog/registry boundary and correctly references `acp-capability-set.md` plus `agent-nexus-tool-bridge.md`. The KB naming choice `nexus.kb_snapshot.read` aligns with the existing catalog row and compound-domain pattern.

## Verdict

**Request Changes**

The cutover itself is clean and build health is good, but P0’s core acceptance is a registry SSOT that P1 can extend without parallel-list drift. The remaining runtime allowlist and the stale, unenforced test-vector metadata undercut that maintainability goal, so this should not be approved until those two medium findings are addressed or explicitly deferred by PM/architect with a tracked residual.
