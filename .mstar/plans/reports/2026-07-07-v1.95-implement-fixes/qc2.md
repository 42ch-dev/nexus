---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-07-v1.95-implement-fixes"
verdict: Approve
generated_at: "2026-07-07T12:00:00Z"
focus: security_correctness
---

# Code Review Report — qc2 (security & correctness)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: security and correctness risk (destructive DB reset, config write integrity, client factory guards, path handling)
- Report Timestamp: 2026-07-07

## Scope
- plan_id: `2026-07-07-v1.95-implement-fixes`
- Review range / Diff basis: `7c61c033..309419bc` (main..HEAD)
- Working branch (verified): `feature/v1.95-implement-fixes`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- branch review-package: `/Users/bibi/workspace/organizations/42ch/nexus/.mstar/sdd/2026-07-07-v1.95-implement-fixes/branch-review.diff`
- Files reviewed: 34 (diff) + targeted source reads in `apps/desktop/src-tauri/src/lib.rs`, `apps/web/src/lib/client-context.tsx`, `apps/web/src/pages/setup-step-welcome.tsx`, `apps/web/src/lib/nexus/desktop-capabilities.ts`, and test files
- Commit range: 6 commits (309419bc..51f956ca)
- Tools run: `git rev-parse`, `git diff --name-only`, `git log`, `grep` (targeted), `read` (source + diff + plan)

## Deep Review
Deep review triggered (high-risk destructive operation + config persistence + client factory security boundary). Lenses applied: Security Lens (path traversal, data deletion scope, injection), Correctness Lens (toml round-trip fidelity, stale-path guard, client-type guard, Option handling).

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S-001 (maintainability)**: `write_workspace_path_at` follows the identical safe `toml_edit` round-trip + `?` parse-error pattern used by `write_setup_completed_at` and `write_agent_profile_at`, both of which have explicit "rejects malformed TOML, original keys survive" tests. No dedicated unit test for the workspace_path writer itself appears in the changed files. Adding one (mirroring the existing malformed tests) would make the contract explicit and protect future edits. (Low risk — implementation is correct.)

## Source Trace
- **Finding ID**: N/A (no blocking findings)
- **Source Type**: git-diff + manual source read + test inspection
- **Source Reference**:
  - `reset_local_database` / `reset_local_database_at`: `apps/desktop/src-tauri/src/lib.rs:397-440` + test `804-842`
  - `set_workspace_path` / `write_workspace_path_at`: `462-487`
  - `pick_directory`: `445-458`
  - ClientProvider `!loaded` + `selectClients`: `apps/web/src/lib/client-context.tsx:81-88,205-217`
  - FingerprintGate `/setup` bypass: `113`, test `224-245`
  - Stale detection: `apps/web/src/pages/setup-step-welcome.tsx:110-117` + tests `100-134`
- **Confidence**: High

## Detailed Security & Correctness Review (per assignment focus)

### 1. `reset_local_database` (HIGHEST RISK — data deletion)
- **Scope**: Only walks `~/.nexus42/creators/*/workspaces/*/`. For each workspace dir, deletes **exactly** the three filenames: `state.db`, `state.db-wal`, `state.db-shm` using `std::fs::remove_file`.
- **No `remove_dir_all`**: Confirmed — only targeted `remove_file`.
- **No arbitrary globs**: Hard-coded exact names in a small array; no `glob` or user-supplied patterns.
- **User workspace untouched**: Test (`reset_local_database_wipes_only_state_db_under_nexus42`) creates `~/Documents/nexus/default/creative.md` outside the `.nexus42` tree and asserts it survives. `wiped == 1` (only the state.db under the simulated creator/workspace).
- **Error handling**: `reset_local_database_at` returns `std::io::Result<usize>` and uses `?` on `read_dir` / `remove_file`. Errors on any deletion stop the operation (fail-fast, no silent partial state). Outer command maps errors to `String`. No `unwrap()` on the IO hot path in the reset functions.
- **Setup-wizard fallback**: When no `active_creator_id`, the walk over `creators/*` naturally covers all candidates (as specified).
- **Verdict**: Correct and safe. The destructive operation is narrowly scoped and the test explicitly proves the "user creative files are safe" contract.

### 2. `set_workspace_path` + `write_workspace_path_at`
- **toml_edit round-trip**: Reads existing text (if present) → `parse::<toml_edit::DocumentMut>()?` → mutate only `workspace_path` → write. Parse failure propagates via `?`; function returns `Err`. `set_workspace_path` surfaces it as `Err(String)`. TS caller in `continueToNext` logs and returns early (user stays on step, can retry). **No fallback to empty document** — no silent wipe of other keys.
- **Malformed TOML test coverage**: The exact same safe pattern is unit-tested for `setup_completed` and `agent_profile` writers (malformed input → error, original keys preserved, new key not written). While a dedicated test for `write_workspace_path_at` is not present in the diff, the implementation is identical.
- **Stale-path detection (TS side)**: `shouldPersistWorkspacePath` returns true only for:
  - explicit `picked === true` (user used Browse…), or
  - `path.includes('Documents/nexus42/')`, or
  - `path.includes('nexus/local/default')`.
- **Custom paths preserved**: Test "does not write the workspace path when it is a custom non-stale path" uses `/Users/x/MyCreative/Nexus` and asserts `setWorkspacePath` was **not** called. A path like `/Users/x/projects` is untouched.
- **Verdict**: Correct. No silent overwrite of user config or custom paths.

### 3. `pick_directory`
- Returns `Result<Option<String>, String>`. `None` on user cancel (`blocking_pick_folder()` returns null → `Ok(None)`).
- The returned path is passed verbatim to `setWorkspacePath` (no string manipulation or injection).
- Browser builds never call it (button hidden when `!desktop`).
- **Verdict**: Clean Option semantics; no path injection surface.

### 4. ClientProvider `!loaded` fix + `isDesktopBuild` guard
- `selectClients()` (module scope, used by tests): `if (!isDesktopBuild()) { Browser } else { Tauri + TauriDesktopCapabilities }`.
- Inside `ClientProvider`:
  - `const isDesktop = useMemo(() => isDesktopBuild(), []);`
  - `if (!loaded) { if (isDesktop) { return Tauri... } else { Browser } }`
  - Loaded path also uses `buildClient(config, isDesktop)`.
- Dedicated test: "selects TauriClient on first render while config is loading in desktop build".
- `isDesktopBuild()` is runtime detection (not build-time constant); browser build returns false.
- **No regression path** for a browser bundle to receive a Tauri client.
- **Verdict**: Guard is present in both the eager factory and the loading branch. Safe.

### 5. FingerprintGate `/setup` bypass
- Change: `isConnectRoute = ... === '/connect' || === '/setup'`.
- Behavior: on `/setup`, the gate does not show verifying/fetch-failed shells and does not redirect on mismatch.
- Rationale (plan + compass): setup wizard runs before any remote config exists; bypass for null config is pre-existing correct behavior.
- **Already-configured remote risk?**
  - The `SetupGate` (separate component) redirects to `/setup` only when `!setup_completed`.
  - Once setup is completed, users do not land on `/setup`.
  - A saved remote config with fingerprint would only be relevant after first setup. Widening the bypass for the wizard route does not open a hole for an already-configured user in normal navigation.
- Test: "treats /setup as a bypass route for the fingerprint gate" — explicitly verifies children render on `/setup` even with a remote config present and fetch failing.
- **Verdict**: Intentional, documented, and safe in context. No new security gap for configured remotes.

### 6. Path traversal / injection in new file-path handling
- `pick_directory`: only uses user-supplied default as dialog starting point; result is the OS-chosen path (or None).
- `set_workspace_path`: writes the string as a TOML value. No filesystem traversal or command construction.
- `reset_local_database`: purely internal scoped walk under `~/.nexus42/creators`; no user-controlled paths.
- No new `..`, symlink, or injection surfaces introduced.
- **Verdict**: No path-traversal or injection issues.

### 7. Other observations (non-blocking)
- The plan itself documents the triplicate `resolve_default_workspace_path` / `default_workspace_root` as known tech debt (`R-V195-ARCH-DUPLICATE-DEFAULTS`). Not in scope for this review.
- Sidecar stderr capture gap is acknowledged in the plan as future work; T4 reset is the pragmatic recovery path.
- All new desktop commands are registered in the invoke handler list.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

The highest-risk change (`reset_local_database`) is correctly scoped, uses only exact filename deletes, never touches the user workspace, and is backed by an explicit survival test. Config write uses safe `toml_edit` round-tripping with parse-error propagation. Client factory and FingerprintGate guards are present and tested. Stale-path overwrite rules preserve custom user paths. No path traversal or injection surfaces were introduced.

One low-severity Suggestion (add a direct malformed-TOML test for the new workspace writer) does not block.

## Revalidation Notes
- All checks performed on verified checkout (branch `feature/v1.95-implement-fixes`, HEAD `309419bc`, cwd matches Review cwd).
- No CI output reviewed (READ-ONLY per QC role constraints); focused on source, diff, and plan alignment.
- No product code edited.

(End of report)
