---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-07-07-v1.95-implement-fixes
verdict: Approve
generated_at: 2026-07-07T00:00:00.000Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Review Perspective: Performance and Reliability Risk
- Report Timestamp: 2026-07-07

## Scope
- plan_id: 2026-07-07-v1.95-implement-fixes
- Review range / Diff basis: 7c61c03320ae4bada10cfe708fe91062b9d81665..fe7a2730099e998f3a5e87b80b537e75560d6091
- Working branch (verified): feature/v1.95-implement-fixes
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: apps/desktop/src-tauri/src/lib.rs, apps/web/src/pages/setup-step-daemon.tsx, apps/web/src/lib/client-context.tsx, apps/web/src/pages/setup-wizard-page.tsx, apps/web/tailwind.config.ts
- Tools run: git diff, file reads

## Findings

### 🔴 Critical
None

### 🟡 Warning
1. **`pick_directory` blocks Tauri main thread**
   - Source: apps/desktop/src-tauri/src/lib.rs:445
   - Issue: Uses `blocking_pick_folder()`, which blocks the main Tauri thread while the native dialog is open. This can cause the app to hang or become unresponsive.
   - Fix: Use the async version of the dialog picker from `tauri-plugin-dialog`.
   - Confidence: High
   - **Status**: Resolved in commit fe7a2730

2. **`reset_local_database` has no atomicity or rollback**
   - Source: apps/desktop/src-tauri/src/lib.rs:404-440
   - Issue: Deletes files in a loop without any atomicity guarantees. A failure mid-deletion (e.g., permission error, disk full) leaves partial state, and there is no rollback mechanism.
   - Fix: Consider using a temporary directory + rename pattern, or at least log the progress and allow partial recovery.
   - Confidence: Medium
   - **Status**: PM-accepted as deferred residual (recovery-wipe semantics; daemon recreates fresh DBs on boot regardless; partial wipe is recoverable; true multi-file-delete atomicity isn't a standard pattern; V1.96 hardening candidate)

3. **`set_workspace_path` writes directly without temp file**
   - Source: apps/desktop/src-tauri/src/lib.rs:468-487
   - Issue: Writes directly to `config.toml`. A crash or interruption mid-write could corrupt the config file.
   - Fix: Use the temp-file-then-rename pattern (write to a temporary file, then rename to the final path atomically). Follow the pattern used elsewhere in the codebase if present.
   - Confidence: Medium
   - **Status**: PM-accepted as deferred residual (matches the existing `write_setup_completed_at` / `write_agent_profile_at` pattern which ALL use `std::fs::write`; codebase-wide temp-file-then-rename hardening is V1.96; fixing only this writer would create inconsistency)

### 🟢 Suggestion
1. **Add timeout to setup daemon step**
   - Source: apps/web/src/pages/setup-step-daemon.tsx:18-60
   - Issue: If the daemon never emits a status (e.g., due to a bug), the wizard will hang indefinitely, relying only on the useEffect cleanup to unsubscribe.
   - Fix: Add a timeout that transitions to an error state after a reasonable duration (e.g., 30 seconds).
   - Confidence: Low

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 1 |

## Revalidation
- Revalidated commit: fe7a2730099e998f3a5e87b80b537e75560d6091
- W1 status: Resolved — `pick_directory` now uses `pick_folder()` with a tokio oneshot channel instead of `blocking_pick_folder()`, yielding to the runtime while the native modal is open.
- W2/W3 status: Acknowledged as PM-accepted deferred residuals (no action required for this round).

**Verdict**: Approve