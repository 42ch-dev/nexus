# nexus-agent-host — Agent Host Core

Hybrid managed-only host core for local agent execution in the Nexus daemon runtime.

## Purpose

This crate provides the orchestration/facade layer above `nexus-acp-host` and native CLI
process adapters. It normalizes ACP providers and native CLI providers behind narrow
`ProviderAdapter`, `HostFacade`, and `ProviderDiscovery` traits.

## Key Rules

- **Reuse `nexus-acp-host`** for all ACP transport, registry, policy, and session primitives.
  Never hand-roll ACP JSON-RPC.
- **Use `nexus-home-layout`** for all filesystem paths. No hardcoded `~/.nexus42/`.
- **Managed-only**: every session and provider process is host-owned, observable,
  cancellable, and shut down by lifecycle hooks. No unmanaged attach mode.
- **ACP-first**: ACP providers are preferred; native CLI providers report honest
  limited capabilities.
- **No placeholder `todo!()` or `unimplemented!()`** in trait impls — use stub modules
  with `mod` declarations only until concrete implementations are built.
- **`HostOperation`** contains only execution-scoped variants: `Prompt`, `SetModel`, `SetMode`.
  Cancel and Health are control-plane actions on `HostFacade`, not operations (R-002).
- **Permission delegation**: ACP provider permissions delegate to `nexus-acp-host::PermissionPolicy`.
  Native CLI providers use host-level risk classification only (R-003).

## Dependencies

- `nexus-acp-host` — ACP client adapter (SDK, registry, transport, policy)
- `nexus-contracts` — generated wire types
- `nexus-home-layout` — path layout helpers

## Module Layout

- `error` — `HostError` enum and `HostResult` type alias
- `ids` — `ProviderId`, `HostSessionId`, `HostOperationId`
- `config` — `AgentHostConfig`, `ProviderConfig`, `TimeoutConfig`
- `core` — `HostManager`, session/op registries, lifecycle
- `capability` — `HostOperation`, `HostEvent`, capability negotiation, risk classification
- `discovery` — config-based, PATH-scan, ACP-registry discovery
- `policy` — admission and permission gates
- `providers` — ACP and native CLI provider adapters
- `telemetry` — structured host event helpers

## Design Reference

See `.agents/knowledge/v1.18-delivery-compass-v1.md` for the authoritative design spec.
