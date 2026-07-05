---
report_kind: code-review
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-26-v1.67-desktop-shell-polish
verdict: Approve
generated_at: 2026-06-26T21:45:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security + correctness risk (path guards, sidecar lifecycle state machine, restart budgeting, event emission)
- Report Timestamp: 2026-06-26T21:45:00Z

## Scope
- plan_id: `2026-06-26-v1.67-desktop-shell-polish`
- Review range / Diff basis: P-sec code commits `cf48a8f1` + `bc8d4bea`, merged at integration HEAD. `git show cf48a8f1 bc8d4bea`; diff basis vs `origin/main`.
- Working branch (verified): iteration/v1.67
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 14 (primary: sidecar.rs, lib.rs, path_guard.rs, runtime_lock.rs, chapters.rs, host_tool_handlers.rs, daemon-status-bar.tsx + tests; plus CI and Cargo files)
- Commit range: cf48a8f1726eaf52588a65b0f9c81b501cd137ea + bc8d4bea1c6ef8bf2dbd06c504eb95da3729ae0f (on top of prior merges to iteration/v1.67 HEAD b3498361)
- Tools run: git show, git diff origin/main, read (source + plan + spec), cargo clippy (scoped to nexus-daemon-runtime + apps/desktop/src-tauri), grep for TOCTOU/guard/unwrap/restart_count/notify

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- (Informational) The TOCTOU comment added in both guards is accurate and appropriately scoped. The race window exists in any canonicalize-then-check pattern on a live FS; for the single-user local desktop/daemon threat model (V1.66/V1.67) it is correctly classified as accepted residual (R-V166-QC2-TOCTOU) rather than a defect to close in this wave. The merged result after P0 + these commits remains coherent: both implementations use the same `canonicalize()` + `starts_with()` (component-wise) discipline; no drift introduced.

## Source Trace

**R-V166-QC2-TOCTOU (path-guard TOCTOU documentation + coherence)**
- Source: bc8d4bea (daemon) + cf48a8f1 (desktop)
- Source Reference: `crates/nexus-daemon-runtime/src/api/path_guard.rs:22-28` (resolve_guarded_path), `apps/desktop/src-tauri/src/lib.rs:110-116` (guard_path)
- Evidence: Both sites now carry the identical paragraph:
  > "There is a small race window between canonicalizing the workspace root and canonicalizing the requested path: a local attacker with filesystem access could replace either path during that window. This guard is authoritative for the single-user local [daemon|desktop] context; adversarial multi-user FS access is out of V1.66/V1.67 scope and tracked by `R-V166-QC2-TOCTOU`."
- Guard logic (post-merge): daemon uses `canonical_root` + `canonical_target.starts_with(&canonical_root)` (must_exist) or parent-probe + starts_with (!must_exist). Desktop guard_path (unchanged logic, only comment added) performs equivalent canonicalize + prefix check before opener calls. P0 edits to daemon handlers did not touch the guard helper; the two sites remain consistent.
- Confidence: High

**R-V166-QC1-S2 (sidecar.rs robust unwrap → explicit)**
- Source: cf48a8f1
- Source Reference: `apps/desktop/src-tauri/src/sidecar.rs:239-258` (start() error path)
- Evidence: Previous `.unwrap()` on `inner.detail` replaced. Error path now:
  ```rust
  let message = if conflict { ... } else { ... };
  inner.state = DaemonState::Error;
  inner.detail = Some(message.clone());
  drop(inner);
  self.notify().await;
  Err(message)
  ```
  No `.unwrap()` or `expect` on the detail Option in the start error branch. Other error paths (spawn failure, health timeout) already used early returns with descriptive strings. No panic surface remains on the documented sidecar lifecycle entry points.
- Confidence: High

**R-V166-QC3-S1 (restart_count reset on manual startDaemon)**
- Source: cf48a8f1
- Source Reference: `apps/desktop/src-tauri/src/sidecar.rs:180-183` (start())
- Evidence:
  ```rust
  inner.state = DaemonState::Starting;
  inner.detail = None;
  inner.stop_requested = false;
  // Manual start (re)sets the crash budget so a previously give-up manager
  // can be recovered by the user (QC3-S1).
  inner.restart_count = 0;
  ```
  Subsequent success path also zeros it. `handle_crash` only increments on actual crash paths and checks `stop_requested` + `restart_count >= MAX_RESTART_ATTEMPTS` before giving up. A manual `startDaemon` after give-up will always reset the counter before the next attempt sequence. No path where manual start bypasses the reset or still hits the cap without a fresh budget.
- Confidence: High

**Event-driven status correctness (no double-emit / missed-transition) — S1**
- Source: cf48a8f1
- Source Reference: sidecar.rs `notify()`, lib.rs setup, daemon-status-bar.tsx subscription
- Evidence:
  - `notify()` is called exactly once per state-changing exit point after the inner lock is dropped (start success, start error, stop, handle_crash give-up/early returns, attached-daemon probe failure paths).
  - React side: one-time `setup()` does initial `refresh()` + single `onDaemonStatusChanged` subscription; cleanup removes listener. No polling loop remains (POLL_MS constant deleted).
  - State machine is fail-closed: `stop_requested` is set before any async work that could race; transitions are driven from the manager only.
  - No double-emit observed: each public API (`start`, `stop`, `status` probe side-effect) emits at most once per call.
  - Initial status is fetched before subscribe; subsequent changes arrive via event. No missed "Running → Degraded" or "Starting → Error" paths in the reviewed call graph.
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 (informational TOCTOU scoping) |

**Verdict**: Approve

## Verification Evidence (executed in this review)
- Branch/HEAD: `iteration/v1.67 @ b3498361` (matches Assignment)
- `git show cf48a8f1 bc8d4bea` — inspected path-guard TOCTOU comments, sidecar restart reset, unwrap removal, notify calls, fsync additions
- Scoped clippy:
  - `cargo clippy -p nexus-daemon-runtime -- -D warnings` → clean
  - `cargo clippy -p nexus-desktop --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings` → clean
- Source reads: path_guard.rs (full), sidecar.rs (lifecycle + notify + backoff + probe_health), lib.rs (guard_path + RunEvent), daemon-status-bar.tsx (event subscription), chapters.rs + host_tool_handlers.rs (dir fsync), runtime_lock.rs (doc correction)
- Desktop shell spec read for path-guard context (`.mstar/knowledge/specs/desktop-shell.md` §9)
- Plan read for residual mapping (`.mstar/plans/2026-06-26-v1.67-desktop-shell-polish.md`)

All four assigned items (R-V166-QC2-TOCTOU, R-V166-QC1-S2, R-V166-QC3-S1, event-driven S1) were directly verified. The merged changes are coherent; no residual race or state-machine defect found within the stated scope and threat model.
