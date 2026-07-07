---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-06-v1.94-closure"
verdict: "Approve"
generated_at: "2026-07-06"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk (PATH scan execution boundary, agent-host/scan endpoint, `setup_completed` config writing, workspace-default path resolution, daemon-ready gate state machine)
- Report Timestamp: 2026-07-06

## Scope
- plan_id: 2026-07-06-v1.94-closure
- Review range / Diff basis: merge-base: bf0e60cc (main HEAD pre-V1.94) + tip: fd9c6d5d (iteration/v1.94 integrated HEAD) ≡ `git diff main...iteration/v1.94`
- Working branch (verified): iteration/v1.94
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 12 (primary: crates/nexus-acp-host/src/registry.rs, crates/nexus-daemon-runtime/src/api/handlers/agent_host.rs + mod.rs, apps/desktop/src-tauri/src/lib.rs + sidecar.rs, apps/nexus42/src/config.rs, schemas/daemon-api/agent-host/*.schema.json, desktop-shell.md §14.3)
- Commit range: bf0e60cc..fd9c6d5d
- Tools run: git diff/merge-base, cargo clippy -p nexus-acp-host -p nexus-daemon-runtime -- -D warnings (clean), cargo test -p nexus-acp-host -p nexus-daemon-runtime (all 37+ unit/integration/doc tests green), desktop src-tauri clippy (clean after sidecar bootstrap)

## Deep Review
Deep review triggered (security-sensitive new execution boundary + new HTTP endpoint + config mutation + daemon gate state machine).  
**Lenses applied:** Security Lens + PATH Execution Lens + Auth Lens (per assignment §2).

## Findings

### 🔴 Critical
- **None.**

### 🟡 Warning
- **None (all mandatory items pass).**

### 🟢 Suggestion
- **S-01 (docs hygiene):** `desktop-shell.md` §14.3 states "≤2s timeout" and "N=4" as the contract. The constants `SCAN_VERSION_TIMEOUT` and `SCAN_MAX_CONCURRENT` are the enforcement points; a one-line comment cross-reference from the spec to the two `const` definitions would make the normative link grep-able for future maintainers. (Low impact; implementation already matches.)
- **S-02 (test coverage note):** The scan endpoint integration test (`scan_endpoint_returns_200_with_frozen_shape`) uses a pre-populated registry cache and `DaemonApiConfig::keyless()` for the test server. Production wiring (in `api/mod.rs`) places `/scan` inside the protected `agent_host_routes()` group behind `require_api_key`. This is correct, but the test does not exercise the auth middleware path. Existing pattern across other agent-host tests; no new risk introduced.

## Source Trace (selected high-confidence items)

| Finding | Source Type | Source Reference | Confidence |
|---------|-------------|------------------|------------|
| PATH binary names from registry only | git-diff + manual | `registry.rs:516-532` (HashSet over `agent.distribution.binary.*.cmd`) | High |
| Bounded concurrency ≤4 | git-diff + manual | `registry.rs:482` (`SCAN_MAX_CONCURRENT=4`), `535` (Semaphore) | High |
| ≤2s --version timeout | git-diff + manual | `registry.rs:485` (`SCAN_VERSION_TIMEOUT`), `591` (`tokio::time::timeout`) | High |
| No shell expansion | git-diff + manual | `registry.rs:586` (`Command::new(binary).arg("--version")`); `which::which` not `sh -c` | High |
| No user-supplied commands in scan | git-diff + manual | `registry.rs:501-503` (scan only walks registry); `launch_command` populated post-scan in wizard | High |
| Handler returns NexusApiError | git-diff + AGENTS.md | `agent_host.rs:537` (Result<..., NexusApiError>), `550` (Internal on registry error) | High |
| Route wired in protected group | git-diff + code | `api/mod.rs:47-49` (inside `agent_host_routes()`); same group as health/providers/sessions | High |
| Response shape matches frozen schema | schema read + code | `scan-response.schema.json` + `AgentScanEntry` construction in `build_scan_entry` | High |
| setup_completed additive (no deny_unknown) | git-diff + code | `config.rs:44` (`#[serde(default)]`), `lib.rs:269-273` (read struct with default) | High |
| Workspace default via dirs + safe create | git-diff + code | `lib.rs:32-44` (`default_workspace_root`), `107-129` (resolve + create_dir_all), `apps/nexus42/src/config.rs:81-93` (matching) | High |
| toml_edit round-trip write (no traversal) | git-diff + code | `lib.rs:281-298` (read → parse → edit → write to `~/.nexus42/config.toml` via home_dir) | High |
| Daemon gate reuses SidecarManager + 15s timeout | git-diff + code | `sidecar.rs:26` (`HEALTH_START_TIMEOUT`), `137` (notify on state change), `334-338` (spawn start) | High |
| No new /v1/local/* literals | grep | No matches in changed .rs files for `/v1/local/` | High |
| New deps license-compatible | Cargo.toml + diff | `which` (MIT/Apache), `toml_edit` (MIT/Apache) — compatible with workspace | High |

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Verification Evidence (QC2 lens)

**1. PATH scan safety boundary (§14.3, 5 constraints) — all satisfied at cited lines**
1. Registry-known binary names only: `registry.rs:516` (`for agent in &registry.agents`), `531` (`binaries.insert(pb.cmd.clone())`).
2. Bounded concurrency: `registry.rs:482` (`const SCAN_MAX_CONCURRENT: usize = 4`), `535` (`Semaphore::new(...)`), `539-543` (acquire before probe).
3. ≤2s timeout: `registry.rs:485` (`const SCAN_VERSION_TIMEOUT = Duration::from_secs(2)`), `591` (`tokio::time::timeout(timeout, cmd.output())`), timeout arm at `610-612` (version=None, still reports installed if PATH hit).
4. No shell expansion: `registry.rs:586` (`Command::new(binary).arg("--version").kill_on_drop(true)`); lookup via `which::which`/`which_in` (lines 581-582).
5. No user-supplied commands during scan: `scan_local_installations` (501) only ever walks registry; `launch_command` in response is derived from registry data or supplied later by wizard step 3 (handler `build_scan_entry` 575-608 never executes arbitrary strings).

**2. Scan endpoint + auth**
- Route: `api/mod.rs:47-49` inside `agent_host_routes()` (same group as other `/v1/daemon/agent-host/*` routes).
- Handler: `agent_host.rs:534-572` returns `Result<Json<ScanResponse>, NexusApiError>`; uses `NexusApiError::Internal` for registry failures (matches daemon-runtime AGENTS.md single-source rule).
- Response: `ScanResponse { agents: Vec<AgentScanEntry> }` — exact match to frozen `scan-response.schema.json` + `agent-scan-entry.schema.json`.
- Auth model: inherits the V1.86/V1.92/V1.93 `require_api_key` + Origin allowlist + header-key + TOFU fingerprint protection already applied to the agent-host router group. No bypass added.
- No extraneous filesystem exposure: response only emits registry fields + `installed`/`version` from the bounded PATH probe.

**3. setup_completed + workspace-default**
- TOML: `CliConfig` uses `#[serde(default)]` on `setup_completed: Option<bool>` (config.rs:44-45). No `deny_unknown_fields`. Additive field does not break existing consumers.
- Tauri commands: `get_setup_completed`/`set_setup_completed` (lib.rs:245-253) delegate to `read_setup_completed_at`/`write_setup_completed_at`.
- Write safety: `write_setup_completed_at` (281-298) uses `dirs::home_dir()` + literal `".nexus42/config.toml"`, `toml_edit` round-trip to preserve other keys, `create_dir_all` on parent only. No user-controlled path components; no symlink escape in the construction.
- Workspace default: `default_workspace_root` (32-44) + `resolve_workspace_root` (107-129) use `dirs::document_dir().or_else(|| dirs::home_dir()...)` then `nexus42/default`, with `create_dir_all`. Matches CLI `resolve_default_workspace_path` exactly. Directory creation is platform-default perms (no 0o777).

**4. Daemon-ready gate state machine**
- Both per-launch (lib.rs:334-338) and wizard step 2 consume the same `SidecarManager` + health-probe path.
- `onDaemonStatusChanged` is the Tauri event `nexus://daemon-status-changed` (sidecar.rs:34, 141).
- Timeout: `HEALTH_START_TIMEOUT = 15s` (sidecar.rs:26) is the same constant used by the existing probe logic; no new infinite-wait path introduced.
- No deadlock observed in concurrent subscribe/start paths (async mutex + emit; status() does a lightweight probe only when needed).

**5. Cross-cutting**
- R-V192SEC-001 (TOFU transport-binding): untouched; no regression in the files under review.
- `/v1/local/*` literals: grep across changed .rs found zero new occurrences (V1.90 rename to `/v1/daemon/*` preserved).
- New dependencies (`which`, `toml_edit`): MIT/Apache-2.0 dual-license, compatible with existing workspace policy.

**6. Static checks**
- `cargo clippy -p nexus-acp-host -p nexus-daemon-runtime -- -D warnings`: clean.
- `cargo test -p nexus-acp-host -p nexus-daemon-runtime`: 37+ tests (unit + integration + doc) all passed.
- Desktop crate: `cargo clippy -- -D warnings` (after `pnpm -w run sidecar` bootstrap): clean.

## Conclusion
All security and correctness gates for the V1.94 P0 surface (PATH execution boundary, new daemon endpoint, config mutation, workspace resolution, daemon gate) pass under the assigned lens. Implementation faithfully realises the frozen contract in `desktop-shell.md` §13–14 and the additive schema. No Critical or blocking Warning findings. Approve.
