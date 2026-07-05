---
plan_id: 2026-06-22-v1.58-reference-cli-and-cross-cut-tests
reviewer: qc-specialist-2
reviewer_index: 2
focus: security-correctness
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
working_branch: iteration/v1.58
diff_basis: 04e14908..1f9ff88a
reviewed_at: 2026-06-22T18:42:00Z
verdict: Approve
---

# QC2 — V1.58 P3 Reference CLI & Cross-Cut Tests — Security/Correctness Review

## Summary

Reviewed CLI surface (`nexus42 creator reference refresh`), daemon IPC dispatch path, `all` iteration, body file write, `--dry-run`, inherited URL security, cross-creator isolation, and network test gating.

**Critical gaps identified**:
- `nexus.reference.refresh` is **not registered** in the host tool registry used by `POST /v1/local/agent-host/internal/tool-executions`. The CLI constructs and posts a valid `ToolExecuteRequest`, but `HostToolExecutor::registry_dispatch` → `admission_pipeline` will reject it as `NOT_SUPPORTED` (Gate 1). The end-to-end CLI → daemon → capability path is non-functional.
- Reference source lookup (`list_references`, `get_by_id`) and refresh dispatch are **global** (no `creator_id` predicate). Any active creator can enumerate all sources across creators and trigger refresh on arbitrary `reference_source_id` values.

Other properties (dry-run early exit, 1000-cap on `all`, streaming 100 MiB body cap, HTTPS+private-IP validation inside capability, `#[ignore]` on network tests) hold. Atomic write lacks `fsync` (durability gap, not fsync-before-rename).

High findings block `Approve`.

## Findings

### High severity

- **H-001: `nexus.reference.refresh` not wired through host tool registry (CLI IPC path dead)**  
  Source: `crates/nexus42/src/commands/creator/reference.rs:330` (posts `{"tool_name":"nexus.reference.refresh", ...}` to `/v1/local/agent-host/internal/tool-executions`), `crates/nexus-daemon-runtime/src/capability_registry.rs:674-691` (only `nexus.registry.refresh` registered), `host_tool_handlers.rs:44` (lookup fails → `BadRequest { code: "NOT_SUPPORTED" }`), `host_tool_executor.rs:231`.  
  `ReferenceRefresh` capability exists and is used by scheduler + direct tests, but no `registry_reference_refresh` wrapper and no entry in `build_registry()`.  
  Result: `cargo test -p nexus42 --test reference_refresh_cli` passes (hermetic dry-run + help), but actual refresh via CLI fails at the daemon boundary. Cross E2E (`cross_reference_refresh_e2e.rs`) bypasses IPC entirely (direct `cap.run()`).  
  **Impact**: Feature does not deliver the stated CLI surface.

- **H-002: Missing creator scoping on reference sources (cross-creator enumeration + trigger)**  
  `list_references(&pool, Some(1000), None)` and `get_by_id(pool, id)` in `reference_source.rs:241-296` and `305-349` use no `WHERE creator_id` / workspace predicate. CLI `run_refresh` and `run_list`/`run_show` resolve only the caller's creator for home path, then perform global DB ops.  
  Daemon-side `ReferenceRefresh::run` does `get_by_id` by ID only (no creator gate).  
  When `with_creator_context` is used (CLI path), body is written under the *dispatching* creator's `~/.nexus42/creators/<caller>/...`, not the source's owner.  
  Admission only checks "active creator exists" (host_tool_handlers.rs:56); no subsequent ownership check on the target `reference_source_id`.  
  **Impact**: Any creator with daemon access can list every source, force network fetches (cost/egress), and pollute their own local body dir with foreign content while mutating shared DB metadata.

### Medium severity

- **M-001: `atomic_write_body` omits fsync before rename (durability)**  
  `reference_refresh.rs:175-184`: `tokio::fs::write(tmp); tokio::fs::rename(tmp, target)`. No `sync_all`.  
  Other atomic writes in the workspace (auto_chronology, rules_layers) correctly do `file.sync_all()?` before rename. On crash between write and rename, body may be lost or partial despite DB update.  
  (Note: size cap + streaming is present and correct at 100 MiB.)

- **M-002: CLI `reference_ref` accepts arbitrary strings (no format / prefix validation)**  
  `run_refresh` treats only the literal `"all"` specially; any other string is passed to `get_reference_by_id`. No `^ref_[a-z0-9]+$` or similar check. Error is stable ("not found"), but early rejection with a typed `INVALID_INPUT` / specific exit code would be stronger. No shell injection surface (uses structured HTTP + JSON).

### Low severity

- **L-001: `reference list` and `reference show` are also global**  
  Same root cause as H-002. Not in P3 diff scope for refresh, but surfaces the same isolation model.

- **L-002: E2E test documentation mismatch**  
  `reference_refresh_cli.rs:5-6` claims "Daemon-dependent refresh dispatch is exercised by the cross-reference E2E test". The E2E test does direct capability construction, not the IPC path. Misleading for future maintainers.

## Security/Correctness Properties Verified

- **Input validation**: Partial. No format check on `reference_ref`; unknown sources rejected late with stable error. No shell metacharacter paths (good).
- **Cross-creator isolation**: **FAILED**. Global DB reads + dispatch by bare ID. Creator context only affects body write target path, not authorization.
- **Dry-run correctness**: **PASS**. Early return in `run_refresh` before `DaemonClient` creation or any DB mutation beyond the initial list for display. Tests assert body unchanged.
- **Body file write atomic + fsync**: Atomic (temp+rename) present; **fsync absent**. 100 MiB streaming cap enforced before extend (`MAX_REFERENCE_BODY_BYTES`); `BodyTooLarge` not used here (different constant from CDN path).
- **HTTPS-only + private-IP block (inherited)**: **PASS** (no bypass). `validate_reference_url` called at `reference_refresh.rs:336` before any `HTTP_CLIENT.get`. 10+ dedicated tests + `is_blocked_ip` coverage. P3 CLI never performs its own fetch.
- **`all` path bounded**: **PASS**. `list_references(..., Some(1000), ...)`; DB layer clamps `limit.clamp(1, 1000)`. Scheduler uses 50/tick. No unbounded dispatch.
- **Network tests gated**: **PASS**. All httpbin tests carry `#[ignore = "requires network access to httpbin.org"]`. One E2E test additionally guards on `NEXUS_TEST_ALLOW_NETWORK`. Not run by default `cargo test`.

## Verdict Reasoning

Two **High** findings:
- The advertised CLI command cannot complete a refresh through the documented daemon IPC path (tool not registered).
- The refresh mechanism has no creator ownership enforcement on the target source.

These are correctness (feature does not work as specified) and security (unauthorized cross-creator action + information disclosure) issues. Per review rules, any High finding requires `Request Changes`.

Medium durability and validation gaps should be addressed in the same fix wave or explicitly waived with rationale before the next gate.

Properties that were in scope and hold (dry-run, caps, URL validation, ignore gating) are noted so the implementer can focus on the blocking items.

## Cross-Plan Concerns

- Reference source table and DAOs were designed without creator/workspace predicates (unlike `findings`, `works`, `novel_pool_entries`, etc.). This pattern reappears in P3. Consider a follow-up to add `creator_id` column (or reliable workspace→creator mapping) + scoped queries if multi-creator isolation is a V1.x requirement.
- Host tool registry population for new `nexus.*` refresh tools needs an explicit checklist item in future plans (compare to how `nexus.registry.refresh` was wired).
- Atomic write helpers should be centralized with fsync (see `auto_chronology::atomic_write` as precedent) to avoid repeated durability misses.

**End of QC2 report.**

## Revalidation

**Revalidated by**: qc-specialist-2
**Revalidated at**: 2026-06-22T08:32:35Z
**Diff basis**: 5cba5235..ba334fa8 (P3 fix-wave)

### Findings Status

| Original Finding | Severity | Status | Evidence |
| --- | --- | --- | --- |
| H-001 host_tool registration | HIGH | Closed | `host_tool_executor.rs:58` (TOOL_ALLOWLIST); `host_tool_handlers.rs:1174` (`execute_reference_refresh` → `ReferenceRefresh::with_pool().with_creator_context`); `host_tool_handlers.rs:1193` (registry wrapper); `capability_registry.rs:694-710` (CapabilityRow); `capability_registry.rs:762` (21 tools test); `cross_caller_e2e.rs:51` (NEXUS_TOOL_IDS); `cargo test -p nexus-daemon-runtime --lib capability_registry` (all 11 pass) |
| H-002 creator scoping | HIGH | Closed | Migration `202606220004_reference_sources_creator_id.sql` (adds `creator_id` + idx); `reference_source.rs:386` (`find_by_id_for_creator` with `AND creator_id=?2`); `reference_source.rs:252` (`list(..., creator_id: Option)`); `reference_refresh.rs:289-300` (uses scoped lookup when `creator_id` present); `reference.rs:269,276,283` (CLI passes creator to all DB paths); `cargo test -p nexus42 --test reference_refresh_cli` (5 pass); `cargo test -p nexus-orchestration --test cross_reference_refresh_e2e` (6 pass) |
| M-001 fsync | Medium | Closed | `reference_refresh.rs:175-192` (`atomic_write_body`: `write` → `File::open` → `sync_all` → `rename`); matches W-001 fix in diff |
| M-002 CLI early-not-found | Medium | Deferred | No format regex added (still accepts any `reference_ref` string); late scoped lookup now returns explicit "not found or not owned" (CLI:286). Not a new early `INVALID_INPUT` as suggested. |
| L-001 docstring | Low | Deferred | `reference_refresh_cli.rs:5-6` docstring still references cross E2E for "daemon-dependent dispatch" (E2E uses direct cap + one creator-context test). Not updated in P3 wave. |

### New Findings (if any)

None.

### Verdict
**Verdict**: Approve
**Rationale**: All original HIGH findings (H-001 host tool registration, H-002 creator scoping) are closed with concrete file+line evidence and passing tests (registry 21 tools, CLI hermetic, cross E2E). M-001 fsync addressed. M-002 and L-001 remain deferred (no new blocking behavior introduced). No new HIGH findings. Per verdict rules: Approve.
