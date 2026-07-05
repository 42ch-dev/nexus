---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-04-20-v1.6-ws-d-multi-preset"
verdict: Request Changes
generated_at: "2026-04-20"
---

# QC #3 Report — V1.6 WS-D Multi-Preset Scheduler

**Review scope**: `git diff ccd6aee..HEAD` (WS-C Done → WS-D HEAD)
**Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`
**Working branch**: `feature/v1.6`
**plan_id**: `2026-04-20-v1.6-ws-d-multi-preset`

---

## Summary

The WS-D implementation correctly introduces a directory-based system preset scanner, a new CLI command, and backward-compatible auto-creation of `_system.maintenance`. However, **one critical behavioral regression** was found in the embedded maintenance preset YAML that will cause the state machine to **block waiting for manual input after the first capability**, breaking the intended linear chain behavior. Additionally, two integration tests were weakened by removing explicit assertions about `_system.maintenance`.

**Verdict: Request Changes** (1 Critical, 2 Warnings)

---

## Critical Findings

### R1 — Behavioral Regression: `exit_when: { kind: rule }` blocks state machine

**Severity**: Critical — functional regression, blocks `_system.maintenance` from completing
**Location**: `crates/nexus-orchestration/src/system_preset_dir.rs`, `EMBEDDED_MAINTENANCE_YAML` constant, lines 197–235

**Problem**:

The embedded `EMBEDDED_MAINTENANCE_YAML` uses `exit_when: { kind: rule }` on intermediate states (`sync_pull`, `outbox_flush`, `registry_refresh`). The `RuleCheckTask` (`tasks/mod.rs:100–129`) reads the `_rule` context variable:

```rust
let rule: String = context.get("_rule").await.unwrap_or_default();
let (passes, reason) = match rule.as_str() {
    "always_true" => (true, ...),
    "always_false" => (false, ...),
    other => (false, format!("unsupported rule: '{other}'")),  // ← empty string hits this
};
let next_action = if passes { NextAction::Continue } else { NextAction::WaitForInput };
```

When `_rule` is not set (the default, as no capability sets it), `passes = false` and `RuleCheckTask` returns `NextAction::WaitForInput`. The state machine **blocks here** and waits for a manual `advance` — it never reaches `outbox_flush`.

The old hardcoded `system_preset.rs::PresetCapabilityTask` (still present but no longer invoked) **always** returns `NextAction::Continue`:

```rust
// system_preset.rs:97-101
Err(e) => Ok(TaskResult::new_with_status(
    Some(format!("{} failed: {}", self.name, e)),
    NextAction::Continue,  // ← always continues, regardless of failure
    Some(...),
)),
```

The plan's own evidence criterion states:
> `nexus42 schedule start _system.maintenance` **identical to pre-V1.6 behavior**

This is violated. The state machine would get stuck after `sync_pull`.

**Evidence chain**:
- `system_preset_dir.rs:197–235` — embedded YAML with `exit_when: { kind: rule }` on every intermediate state
- `tasks/mod.rs:104–119` — `RuleCheckTask` returns `WaitForInput` when `_rule` is unset
- `tasks/mod.rs:581–585` — `StateCompositeTask` calls `RuleCheckTask` for `ExitWhen::Rule`
- `system_preset.rs:94–95` — old `PresetCapabilityTask` always returns `Continue` regardless of capability outcome

**Fix options** (pick one):

1. **Remove `exit_when` from intermediate states** — simplest. Only `end` needs `terminal: true`. When `exit_when` is absent and `terminal` is false, `StateCompositeTask` returns `NextAction::Continue` (`tasks/mod.rs:571–572`). The YAML for intermediate states would be:
   ```yaml
   - id: sync_pull
     description: "Pull remote sync bundles"
     enter:
       - kind: capability
         name: sync.pull
     # no exit_when — falls through to Continue
     next: outbox_flush
   ```

2. **Set `_rule = "always_true"` before rule check** — requires adding a context set step before the `RuleCheckTask` evaluation. More complex; not recommended.

**Impact if not fixed**: `_system.maintenance` never completes. Every scheduled run gets stuck waiting for manual input after `sync_pull`. All subsequent capabilities (`outbox_flush`, `registry_refresh`) are never invoked.

---

## Warning Findings

### W1 — Relaxed integration test assertions for `_system.maintenance`

**Severity**: Warning — test coverage gap for backward compatibility
**Location**: `crates/nexus42d/tests/orchestration_http_smoke.rs`, `crates/nexus42d/src/api/handlers/orchestration/presets.rs`

**Problem**:

Two integration tests that previously asserted `_system.maintenance` is registered have been changed to remove or weaken those assertions:

1. `get_presets_returns_system_maintenance` (smoke test) — previously asserted `resp.presets.iter().any(|p| p == "_system.maintenance")`. This assertion is **gone**. The test sets up `_system/maintenance/` directory but does not verify the preset appears in the response.

2. `list_presets` (handler test) — same assertion was removed, replaced with a comment:
   ```rust
   // _system.maintenance should be auto-created by ensure_maintenance_preset
   // if the scan runs (depends on test environment), but we don't assert it
   // here because the test workspace may not have the directory set up.
   ```

The plan's own evidence checklist requires:
> Engine startup with `_system/maintenance/`: preset registered and functional.

If `ensure_maintenance_preset` silently fails (e.g., permission error on first start), the test suite would not catch it. The weakened assertions reduce confidence in the backward-compatibility story.

**Fix**: Restore explicit assertions that `_system.maintenance` appears in the presets list after the scan. The test environment now properly sets up `nexus_home` via `create_test_workspace()`, so the directory should be created and found.

**Cross-reviewer note**: QC #1 or QC #2 may have noted this; check `qc1-ws-d-multi-preset.md` / `qc2-ws-d-multi-preset.md` for alignment.

---

### W2 — Unused `system_preset` module is dead code

**Severity**: Warning — code cleanliness / future confusion
**Location**: `crates/nexus-orchestration/src/system_preset.rs`

**Problem**:

`system_preset.rs` (the hardcoded Rust graph builder for `_system.maintenance`) is **no longer imported or called** by `nexus42d/src/main.rs`. The WS-D startup code replaced:
```rust
// OLD (WS-C, now removed):
let sys_graph = system_preset::build(capabilities.clone());
concrete_engine.start_session("_system.maintenance", sys_graph).await;

// NEW (WS-D):
system_preset_dir::ensure_maintenance_preset(&system_presets_dir)?;
let scan_result = system_preset_dir::scan_system_presets(...);
for entry in &scan_result.presets { ... }
```

The old module is still exported from `nexus-orchestration/src/lib.rs` and compiles, but no code path uses it. This is dead code that can confuse future developers who may not realize there are two implementations of `_system.maintenance`.

**Fix**: Either remove the `system_preset` module entirely (with a note that WS-D superseded it), or add a `#[deprecated(since = "1.6", note = "Use system_preset_dir instead")]` attribute if it serves as a reference implementation.

---

## Observation Findings

### O1 — `CapabilityRegistry::with_builtins()` called fresh in `list_presets` handler

**Severity**: Observation
**Location**: `crates/nexus42d/src/api/handlers/orchestration/presets.rs:31`

The `list_presets` handler creates a brand-new `CapabilityRegistry::with_builtins()` for the directory scan, rather than reusing `state.capability_registry()`. This is functionally correct (builtins are deterministic and identical), but it creates an extra registry allocation per HTTP request. Not a bug, just minor inefficiency. Low priority.

---

### O2 — `LoadedPreset::clone()` added for preset wiring

**Severity**: Observation
**Location**: `crates/nexus-orchestration/src/preset/loader.rs:22`

`#[derive(Clone)]` was added to `LoadedPreset`. This is necessary for the WS-D flow where `entry.loaded` is cloned into the `build_wired_outer_graph` call. The clone is shallow (contains `Arc<Graph>` etc.), so this is correct and cheap. No action needed.

---

### O3 — `GraphFlowEngine::clone()` added

**Severity**: Observation
**Location**: `crates/nexus-orchestration/src/engine.rs:508–515`

`Clone` was manually implemented for `GraphFlowEngine` (not derived — `EngineSharedState` and `CapabilityRegistry` are `Arc`-wrapped so manual impl is correct). This enables the WS-D flow where `Arc::new(concrete_engine.clone())` is passed to each preset's graph wiring. Correct.

---

## Verification Evidence

| Check | Result | Evidence |
|-------|--------|----------|
| `cargo clippy --package nexus-orchestration --package nexus42 --package nexus42d` | ✅ Pass (0 warnings) | Full build + clippy in 17.61s |
| `cargo fmt --check` (stable) | ⚠️ Diffs only in `crates/nexus-contracts/src/generated/` | Pre-existing; `generated/` excluded by `.rustfmt.toml` nightly-only ignore list; unrelated to this PR |
| `cargo +nightly fmt --check` | ⛔ Not permitted (bash permission) | Pattern `cargo fmt --check*` doesn't match `+nightly fmt --all -- --check` |
| `cargo test` | ⛔ Not permitted (bash permission) | No `cargo test*` pattern in bash allowlist |
| Schema changes | ✅ None | `git diff ... -- '**/schemas/**'` empty |
| Codegen required | ✅ No | No schema changes in diff |

---

## Cross-Reviewer Alignment Notes

This reviewer (QC #3) has identified a **critical behavioral regression** (R1) that other reviewers may have missed because it requires tracing the runtime semantics of `RuleCheckTask` vs `PresetCapabilityTask`. Specifically:

- QC #1 / QC #2 reports (already in `reports/2026-04-20-v1.6-ws-d-multi-preset/`) should be checked for whether they flagged R1.
- If this finding is **novel to QC #3**, PM should request a re-review from the other two reviewers to confirm the analysis.
- **Runtime impact**: If merged as-is, `_system.maintenance` sessions will hang after `sync_pull`. Every scheduled run will require manual `advance` input to proceed — a complete functional break of the maintenance preset.

---

## Plan Update Recommendation

**Gate**: `Request Changes` — Critical R1 must be fixed before approval.

PM to update `status.json` with:
```json
"metadata": {
  "residual_findings": {
    "2026-04-20-v1.6-ws-d-multi-preset": [
      {
        "id": "R1",
        "severity": "Critical",
        "title": "exit_when rule causes state machine to block",
        "detail_doc": ".mstar/plans/reports/2026-04-20-v1.6-ws-d-multi-preset/qc3-ws-d-multi-preset.md"
      },
      {
        "id": "W1",
        "severity": "Warning",
        "title": "Integration tests weakened — _system.maintenance not asserted",
        "detail_doc": ".mstar/plans/reports/2026-04-20-v1.6-ws-d-multi-preset/qc3-ws-d-multi-preset.md"
      }
    ]
  }
}
```
