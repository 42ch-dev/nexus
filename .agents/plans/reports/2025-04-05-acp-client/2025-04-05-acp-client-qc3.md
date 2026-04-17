---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2025-04-05-acp-client"
verdict: REQUEST CHANGES
generated_at: "2026-04-06"
---

# QC Review #3: ACP Client Integration

**Reviewer**: @qc-specialist-3
**Date**: 2026-04-06
**Branch**: feature/v1.0-acp-client
**Commits reviewed**: 3 (ddf7c62, 786bb7c, 3cee742)

## Summary

The implementation follows the architecture spec in `.agents/plans/archived/knowledge/acp-client-tech-spec-v1.md` (archived 2026-04-17 — current design in `knowledge/acp-client-tech-spec-v2.md`) with correct module structure and appropriate use of the adapter pattern. However, **the ACP SDK integration is incomplete** — `AcpSdkAdapter` methods are placeholders with `TODO` comments for the `LocalSet` thread integration that the spec §2.3 identifies as critical for handling `!Send` futures. This is a **blocking issue** because the V1.0 ACP client cannot actually communicate with agents until the full SDK integration is implemented.

**Progress assessment**: According to `status.json`, the plan is at 42% with Tasks 1, 2, 5 done and Tasks 3, 4, 6 remaining. The code diff shows `agent.rs` and `transport.rs` exist, suggesting partial implementation of Tasks 3 and 4, but the SDK adapter remains a stub.

## Findings

### Critical (must fix before merge)

- **[ACP-C3-1] AcpSdkAdapter is a placeholder — SDK integration incomplete**

  **Location**: `crates/nexus42/src/acp/client.rs` lines 356–480

  **Evidence**: All trait methods (`initialize`, `create_session`, `prompt`, `cancel`) return placeholder responses wrapped in `tracing::warn!` with `TODO` comments. The `subscribe()` method calls `unimplemented!()`.

  ```rust
  // client.rs:364-367
  tracing::warn!(
      agent_id = %agent_id,
      "AcpSdkAdapter::initialize() — full SDK integration pending LocalSet thread implementation"
  );
  // Placeholder response...
  ```

  The spec §2.3 explicitly states: "The `tokio::task::LocalSet` requirement: ACP SDK futures are `!Send`, requiring `spawn_local`. The CLI's `#[tokio::main]` creates a multi-threaded runtime by default. We must use `tokio::task::LocalSet` within the agent session to bridge this gap."

  **Impact**: Without the `LocalSet` thread integration, `nexus42 agent run` cannot establish actual ACP sessions with agents. This is the core functionality of the entire ACP client feature.

  **Fix required**: Implement the `LocalSet` thread pattern as documented in the code comments and spec §2.3. This is a prerequisite for Tasks 3 and 4 to be functional.

- **[ACP-C3-2] Stream subscription returns unimplemented**

  **Location**: `crates/nexus42/src/acp/client.rs` line 430

  **Evidence**:
  ```rust
  fn subscribe(&self) -> StreamReceiver {
      unimplemented!("StreamReceiver requires active connection — pending LocalSet integration")
  }
  ```

  **Impact**: Interactive agent sessions cannot receive streamed responses from agents. This breaks the `session/prompt` streaming functionality described in spec §2.3.

### High (should fix before merge)

- **[ACP-H3-1] skills.rs capability builder not wired to initialize**

  **Location**: `crates/nexus42/src/acp/skills.rs` lines 92–98, `crates/nexus42/src/acp/client.rs` lines 369–376

  **Evidence**: `build_v1_0_capabilities()` exists and is tested, but `AcpSdkAdapter::initialize()` returns `AgentCapabilities::default()` instead of calling the capability builder.

  The spec §5.2 (Capability ID Registry) defines frozen IDs and §6 (Task 5) states "include capabilities in initialize request". Task 5 is marked done, but the wiring is missing.

  **Impact**: Agents won't know what capabilities nexus42 supports during handshake.

  **Fix required**: Wire `build_v1_0_capabilities()` into the `InitializeRequest` construction.

- **[ACP-H3-2] Agent spawn uses raw stdio without SDK framing**

  **Location**: `crates/nexus42/src/commands/agent.rs` lines 288–296, `crates/nexus42/src/acp/transport.rs` lines 141–172

  **Evidence**: The `cmd_run` function spawns a subprocess and extracts stdin/stdout pipes directly via `spawner.spawn()`, but never connects them to the `AcpSdkAdapter`. The code shows:
  ```rust
  let (child, _stdin, _stdout) = spawner.spawn(...).map_err(...)?;
  // child is never used with AcpSdkAdapter
  ```

  **Impact**: The ACP JSON-RPC protocol framing is not being used. The subprocess output goes to nowhere ( `_stdin` and `_stdout` are dropped).

  **Fix required**: The `LocalSet` integration (ACP-C3-1) will resolve this by wiring the pipes to the SDK's `ClientSideConnection`.

### Medium (recommend fixing)

- **[ACP-M3-1] Schema codegen produces flat types but registry uses hand-written types**

  **Observation**: The schema `schemas/acp-runtime/registry-manifest.schema.json` defines rich nested types (`AgentEntry`, `Distribution`, `NpxDistribution`, `BinaryDistribution`, `PlatformBinary`), but the generated TypeScript (`packages/nexus-contracts/src/generated/RegistryManifest.ts`) has `agents: string[]` and Rust (`crates/nexus-contracts/src/generated/registry_manifest.rs`) has `agents: Vec<serde_json::Value>`.

  **Not a bug**: Spec §3 "Design Notes" explicitly acknowledges this: "The Rust types for registry data are defined here (not via codegen) because: codegen pipeline only produces flat structs, not nested types; We need proper typed fields for agent distribution."

  **Registry.rs correctly uses hand-written types** that match the schema structure. This is the right approach.

  **Recommendation**: Document this divergence clearly so future maintainers don't try to "fix" it. Consider adding a comment in the generated codegen that the registry schema uses hand-written types.

- **[ACP-M3-2] Agent command missing graceful shutdown integration**

  **Location**: `crates/nexus42/src/commands/agent.rs` lines 300–320

  The code sets up `cancel_tx` for graceful shutdown but the actual agent subprocess is not managed through the `AcpSession` struct from `transport.rs`. The `AcpSession` struct (defined in `transport.rs`) has `graceful_shutdown()` and `kill()` methods that aren't being used.

  **Impact**: If an agent hangs or the user presses Ctrl+C, cleanup may not be graceful.

- **[ACP-M3-3] SimpleClientHandler auto-grant has no logging of granted tool**

  **Location**: `crates/nexus42/src/acp/client.rs` lines 196–214

  The `request_permission` handler auto-grants with a `tracing::warn!` that mentions "Auto-granting permission request" but doesn't log which specific permission/tool was granted. For V1.0 debugging, it would be helpful to log the full `tool_call` details.

  **Recommendation**: Change from `tracing::warn!` to include full tool details:
  ```rust
  tracing::warn!(agent_id = %self.agent_id, tool_call = ?args.tool_call, "Auto-granting permission");
  ```

### Low/Suggestion

- **[ACP-L3-1] Double "Starting" message in cmd_run**

  **Location**: `crates/nexus42/src/commands/agent.rs` lines 285–286

  ```rust
  eprintln!("Starting {} {}...", agent.name, agent.version);
  eprintln!("  Command: {} {}", program, args.join(" "));
  ```

  The "Starting" message is already printed, then "Command" is printed. This could be combined or the wording adjusted for clarity.

- **[ACP-L3-2] test_build_v1_0_capabilities_returns_non_default is a no-op**

  **Location**: `crates/nexus42/src/acp/skills.rs` lines 139–145

  ```rust
  #[test]
  fn test_build_v1_0_capabilities_returns_non_default() {
      let _caps = build_v1_0_capabilities();
      assert!(true, "Capability builder executed successfully");
  }
  ```

  The assertion always passes. Consider removing this test or making it meaningful by checking the returned `ClientCapabilities` has the expected V1.0 capability IDs.

### Cross-Reviewer Ready Notes

The following items should be verified by other reviewers or discussed in consolidation:

1. **Placeholder status vs. plan progress**: Status.json shows Tasks 1, 2, 5 done and Tasks 3, 4, 6 remaining. The code diff shows `agent.rs` (861 lines) and `transport.rs` (515 lines) exist. If these commands are functionally complete (not just skeletons), the status.json progress percentage may be stale.

2. **Runtime impact if ACP-C3-1 is not fixed**: The entire ACP client feature is non-functional. If merged as-is, running `nexus42 agent run` would spawn a subprocess but never be able to communicate with it. This is a complete feature failure, not a partial degradation.

3. **Revert urgency if issues not resolved**: HIGH — If this branch is merged without fixing ACP-C3-1, the ACP client feature will be broken. Rolling back would affect the new `nexus42 agent` command tree and ACP-specific error types.

## CI/CD Observations

- **CI workflow modifications** (`.github/workflows/ci.yml`): Added `cargo test -p nexus42 --test acp_registry --test cli_agent` which correctly targets the new integration tests. The dependency ordering (rust-checks → rust-tests) is correct.

- **Codegen artifact upload/download**: The workflow uploads generated types and downloads them in subsequent jobs. This is correct for the workspace structure.

- **No schema validation job for ACP schemas**: The `validate-schemas` job runs `node tooling/validation/schema-validator.js` but only validates schemas under `tooling/validation/`. The new `schemas/acp-runtime/registry-manifest.schema.json` may not be included in validation. **Verify**: Check if `schemas/acp-runtime/` is included in schema validation.

## Architecture Consistency Check

| Spec Requirement | Implementation | Status |
|-----------------|----------------|--------|
| §2.2: NexusAcpClient trait + AcpSdkAdapter | `client.rs` trait + struct | ✅ Implemented (placeholder) |
| §2.2: Module layout (client, registry, skills, error, transport) | `acp/mod.rs` re-exports | ✅ Correct |
| §2.3: LocalSet for !Send futures | TODO in code | ❌ Not implemented |
| §2.3: 30s initialize timeout, 5m prompt timeout | Not configurable yet | ⚠️ Deferred |
| §3: Registry cache at $HOME/.nexus42/registry/ | `registry.rs` CacheMeta + file I/O | ✅ Implemented |
| §3: Stale-while-revalidate (24h) | `RegistryCache::get_registry()` | ✅ Implemented |
| §5: Frozen capability IDs | `skills.rs` constants | ✅ Implemented |
| §5: build_v1_0_capabilities() | Function exists | ❌ Not wired |
| §6: `nexus42 agent list/show/run/probe` | `agent.rs` with clap | ✅ Structure complete |
| §7: registry-manifest.schema.json | `schemas/acp-runtime/` | ✅ Created |

## Verdict

**REQUEST CHANGES**

The implementation is structurally sound and follows the spec's architecture, but the core ACP SDK integration is incomplete. The `AcpSdkAdapter` is a placeholder that cannot perform actual ACP communication with agents. This is a **functional blocking issue** — the feature will not work as intended.

### Required before merge:
1. **ACP-C3-1**: Implement LocalSet thread integration for `AcpSdkAdapter`
2. **ACP-H3-1**: Wire `build_v1_0_capabilities()` into initialize request

### Recommended before merge:
3. **ACP-H3-2**: Connect agent subprocess stdio to SDK connection
4. **ACP-M3-2**: Use `AcpSession` graceful shutdown in `cmd_run`

### Post-merge residuals (already tracked):
- ACP-R3..R11 (V1.1+ capabilities, permissions, session persistence)
