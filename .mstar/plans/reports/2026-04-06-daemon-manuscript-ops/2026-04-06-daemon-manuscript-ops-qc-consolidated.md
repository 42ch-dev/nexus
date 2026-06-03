---
report_kind: qc-consolidated
plan_id: "2026-04-06-daemon-manuscript-ops"
generated_at: "2026-04-07"
consolidated_by: "@project-manager"
---

# QC Consolidated Decision: Daemon + Manuscript Operations (Plan C)

## Decision: Request Changes

## Blocking Items (Must Fix Before Merge)

| ID | Severity | Location | Issue | Owner |
|----|----------|----------|-------|-------|
| QC3-C1 | **Critical** | `manuscript/manager.rs:59-71` | Path traversal — `manuscript_dir()`, `manuscript_file()`, `metadata_file()` use unsanitized `title` directly in paths. Title `../../../etc` writes outside Stories. | @fullstack-dev |
| QC3-H1 | **High** | `commands/daemon.rs:51-174` | PID file never written by `start_daemon()`. `stop_daemon()` always falls through to fragile `lsof` fallback, potentially killing wrong process. | @fullstack-dev |
| QC3-H2 | **High** | `commands/manuscript.rs:92` | Unsanitized title in temp path — `format!(".nexus42-edit-{}", title)` allows `../` to escape `/tmp`. | @fullstack-dev |

## Non-Blocking Findings (Residuals)

| ID | Severity | Source | Issue | Decision |
|----|----------|--------|-------|----------|
| FIND-2 | Medium | QC-#2 | PID file TOCTOU race condition | Accept — window is small; improve in V1.1 |
| QC3-M1 | Medium | QC-#3 | SIGTERM re-sent in wait loop | Defer — functional, minor efficiency |
| QC3-M2 | Medium | QC-#3 | lsof fallback fragile | Accept — fallback only used when PID file missing |
| QC3-M3 | Medium | QC-#3 | `$EDITOR` not validated | Defer V1.1 |
| QC3-M4 | Medium | QC-#3 | PID file uses default permissions | Defer V1.1 — add 0600 |
| QC3-M5 | Medium | QC-#3 | `extract_references` succeeds on missing ID | Accept — graceful degradation |
| FIND-3 | Low | QC-#2 | Sync status route lacks architecture decision comment | Accept |
| FIND-4 | Low | QC-#2 | Plain text export markdown stripping fragile | Defer V1.1 |
| FIND-5 | Low | QC-#2 | Config parameter unused | Accept — forward-compat placeholder |
| FIND-6 | Low | QC-#2 | Domain error conversion loses specificity | Defer V1.1 |
| QC3-L1 | Low | QC-#3 | SIGTERM sent twice at stop | Accept — harmless |
| QC3-L2 | Low | QC-#3 | UUID-based cache key without dedup | Accept |

## Assigned Fix Owners
- **@fullstack-dev** — fix QC3-C1, QC3-H1, QC3-H2 on `feature/v2.0-daemon-manuscript-ops`

## Next Step
Fix 3 blocking items → re-verify → QC re-review → QA verification → merge to main
