---
plan_id: 2026-06-22-v1.58-reference-cli-and-cross-cut-tests
reviewer: qc-specialist
reviewer_index: 1
focus: architecture-maintainability
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
working_branch: iteration/v1.58
diff_basis: 04e14908..1f9ff88a
reviewed_at: 2026-06-22T16:05:00Z
verdict: Approve
report_kind: qc
---

# QC1 — V1.58 P3 Reference CLI & Cross-Cut Tests — Architecture/Maintainability Review

## Summary

| Severity | Count |
|----------|-------|
| High     | 0     |
| Medium   | 0     |
| Low      | 1     |
| Suggestion | 2   |

**Verdict**: Approve — no architecture or maintainability blocking issues found.

## Findings

### High severity

None.

### Medium severity

None.

### Low severity

#### L-01: Plan file documents abbreviated CLI path without `creator` prefix

**Source**: Plan file `2026-06-22-v1.58-reference-cli-and-cross-cut-tests.md` (lines 22, 28, 42) refers to the subcommand as `nexus42 reference refresh [work_ref|all]`.

**Observation**: The actual implementation places `ReferenceCommand::Refresh` under the `CreatorCommand::Reference` noun group, yielding the full path `nexus42 creator reference refresh [ref_id|all] [--dry-run]`. The spec overlay (`cli-spec.md` §6.2N) correctly documents the full three-plane path. The plan file's abbreviated form could cause confusion for future maintainers who verify behavior against the plan.

**Recommendation**: Update the plan file's CLI path references to include the `creator` command plane prefix, matching the spec overlay and implementation.

### Suggestion

#### S-01: Unused `test_config` helper function in CLI tests

**Source**: `crates/nexus42/tests/reference_refresh_cli.rs` (line 14, annotated `#[allow(dead_code)]`).

**Observation**: `test_config()` is a dead code helper for constructing a mock `CliConfig`. The current tests bypass it (they use `fresh_pool_with_refs` and exercise DB logic directly, never reaching the daemon-call path). The function is useful scaffolding for future daemon-dependent tests but is currently unreferenced.

**Recommendation**: Either remove the dead code (`#[cfg(test)]` won't help here since it's already in a test file) or add a simple use-site to keep it live. Not blocking.

#### S-02: Exit code documentation could clarify partial-failure case

**Source**: `.mstar/knowledge/specs/cli-spec.md` §6.2N (exit codes section).

**Observation**: The spec documents exit code `0` ("all refreshes dispatched successfully") and exit code `1` ("daemon not reachable"). The `run_refresh` implementation handles individual source failures gracefully (continues the batch loop, prints errors) and returns `Ok(())` regardless — effectively exit code 0 even when some individual refreshes fail. This is reasonable batch-command behavior but the spec could document the partial-failure exit code semantics explicitly.

**Recommendation**: Add a note to §6.2N: "If the daemon is reachable but one or more individual source refreshes fail, the command returns exit code 0; per-source errors are printed to stderr."

## Architecture Properties Verified

### CLI three-plane IA coherence

- Verified: `ReferenceCommand::Refresh` sits under `CreatorCommand::Reference`, matching the established three-plane IA pattern (`creator > reference > refresh`). The same hierarchy is used by `kb`, `world`, `knowledge`, `soul`, `memory` — all subcommands of `creator`. No top-level command pollution.
- Verified: `reference` noun group was already established in V1.26 (Register, List, Show); Refresh extends it with a new variant, not a standalone top-level command.

### CLI → daemon → registry path matches V1.57 3-caller adapter pattern

- Verified: `run_refresh()` → `DaemonClient::post("/v1/local/agent-host/internal/tool-executions", ...)` — uses the same host-call endpoint as `host-call` (documented in §6.2M).
- Verified: `CapabilityRegistry` registers `ReferenceRefresh` in all three constructors (`with_builtins`, `with_builtins_and_pool`, `with_runtime_deps`) — no new registration path added.
- Verified: The IPC payload shape (`tool_name: "nexus.reference.refresh"`, `parameters: {reference_source_id}`) matches the capability's `run()` parse schema.
- Verified: Daemon health check is performed before any dispatch (`DaemonClient::health_check()`), with canonical remediation message on failure.

### Body file write atomicity (P1→P3 gap closure)

- Verified: `atomic_write_body()` writes to `<target>.tmp`, then `tokio::fs::rename()` — same temp+rename pattern used by `ScaffoldTransaction` (V1.55 P3).
- Verified: On rename failure, the warning is logged at `warn!` level with structured fields (`reference_source_id`, `body_path`, `error`).
- Verified: `with_creator_context()` is a builder extension — does not modify the existing `new()` / `with_pool()` constructors, so P1's pool-less registration path is not affected.
- Verified: The scheduler path (no creator context) correctly skips disk write and updates only the DB.

### Security (H-001 carry-forward)

- Verified: `validate_reference_url()` enforces HTTPS-only + private-IP blocking (static IP check + DNS resolution + mapped IPv6 check). Covers `fc00::/7` unique-local range.
- Verified: 100 MiB body size limit with streaming chunk enforcement prevents OOM.
- Verified: Streaming body fetch (`bytes_stream()`) with incremental blake3 hashing avoids loading entire response into memory (F-001 fix).

### Spec overlay coherence and completeness

- **`cli-spec.md` §6.2N**: Follows §6.2M (`host-call`) format with field table, exit codes, scope filtering, dry-run semantics, wiring diagram, IPC timeout, and atomic write documentation. Complete.
- **`daemon-runtime.md` §10**: Covers overview (10.1), configuration (10.2 — knobs, defaults, env overrides), query logic (10.3 — filter criteria, ordering, concurrency guard), dispatch path (10.4 — ASCII call sequence), error handling (10.5 — non-abort, counters), and tracing contract (10.6). Complete.
- **`local-runtime-boundary.md` §3.4**: Topology diagram updated with reference refresh caller path from CLI → DaemonClient → HostToolExecutor → admission_pipeline → CapabilityRegistry dispatch → ReferenceRefresh.run. Coherent with existing diagram layout.

### Cross-plan regression (P0/P1/P2)

- Verified: P0 workspace OCC hardening — not touched. No workspace path changes.
- Verified: P1 `ReferenceRefresh` capability registration — unchanged; P3 adds `with_creator_context()` builder method only.
- Verified: P2 DF-56 routing/DAG residuals — not touched.
- Verified: `cargo test -p nexus42 --test reference_refresh_cli` — 5/5 pass.
- Verified: `cargo test -p nexus-orchestration --test cross_reference_refresh_e2e` — 6/6 pass, 2 network-dependent ignored.
- Verified: `cargo clippy -p nexus42 -- -D warnings` — clean.
- Verified: `cargo clippy -p nexus-orchestration -- -D warnings` — clean.
- Verified: `cargo +nightly fmt --all -- --check` — clean.

### Surgical change discipline

- All changes are confined to exactly 7 files (3 source + 2 test + 3 spec overlays). No piggyback refactoring, no unrelated modifications.
- The `#[allow(clippy::too_many_lines)]` on `ReferenceRefresh::run()` is justified — the method is a straightforward state machine and decomposition would add more complexity.
- The `#[allow(dead_code)]` on `test_config` is borderline (see S-01) but not a regression.

## Verdict Reasoning

The implementation is architecturally sound. The `creator reference refresh` CLI subcommand correctly extends the existing `reference` noun group under the established three-plane IA. The daemon IPC path follows the V1.57 3-caller adapter pattern (`DaemonClient::post` → host-call endpoint → `CapabilityRegistry::dispatch`). The body file write (P1→P3 gap closure) uses the correct temp+rename atomic pattern. Security controls (HTTPS-only, private-IP blocking, streaming, 100 MiB limit) are comprehensive. Spec overlays for all three documents are complete and coherent with existing sections.

No High or Medium findings. One Low finding (plan file CLI path inconsistency) and two Suggestions (unused test helper, exit code documentation). None block approval.

## Cross-Plan Concerns

- `capability-registry.md` was listed in the plan's `spec_refs` but no amendment was produced. The plan's `Spec changes` section says "Amend (if P3 adds new capability invocation paths)". Since P3 does not add new capability registry paths (the CLI dispatches through the existing host-call endpoint → existing registry path), this omission is correct. No action needed.
- T8 (`--reference-policy` flag) was intentionally deferred. Ensure it is captured in a residual or backlog entry so it is not silently lost.
- T7 (schema codegen) was intentionally skipped because P1 added no capability enum variants. No action needed.
