---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-07-07-v1.95-implement-fixes
verdict: Request Changes
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
- Review range / Diff basis: 7c61c03320ae4bada10cfe708fe91062b9d81665..309419bcab7f70ef33aa224e03d01cf9af9af321
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

2. **`reset_local_database` has no atomicity or rollback**
   - Source: apps/desktop/src-tauri/src/lib.rs:404-440
   - Issue: Deletes files in a loop without any atomicity guarantees. A failure mid-deletion (e.g., permission error, disk full) leaves partial state, and there is no rollback mechanism.
   - Fix: Consider using a temporary directory + rename pattern, or at least log the progress and allow partial recovery.
   - Confidence: Medium

3. **`set_workspace_path` writes directly without temp file**
   - Source: apps/desktop/src-tauri/src/lib.rs:468-487
   - Issue: Writes directly to `config.toml`. A crash or interruption mid-write could corrupt the config file.
   - Fix: Use the temp-file-then-rename pattern (write to a temporary file, then rename to the final path atomically). Follow the pattern used elsewhere in the codebase if present.
   - Confidence: Medium

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

**Verdict**: Request Changes