# nexus-capability — Private Capability Port Contract

The single owner of the capability port and error contract: the `Capability`
trait, `CapabilityError`, `CapabilityOrigin`, `ToolPolicy`, the prompt port
(`PromptExecutor`, `PromptRequest`, `PromptResult`,
`PromptPermissionScope`), the workspace ports (`WorkspaceExecutor`,
`WorkspaceStateProvider`), `DaemonToolDispatch`, and
`resolve_session_cancellation`. The registry, builtins, admission/scan/watch
and quality-loop scheduling still live in `nexus-orchestration` until they
migrate in later slices.

## Key Rules

- **Private, not SDK.** `publish = false`. It is not part of any public module
  SDK or wire schema, and no consumer may re-export it as a compatibility
  shim; every consumer imports these items directly from `nexus-capability`.
- **Ports only.** No engine, WASM host or executor implementation lives here;
  the real production `PromptExecutor` remains daemon-owned. Implementations
  and the registry stay with `nexus-orchestration` and its consumers.
- **Same signatures, same variants.** Port signatures and `CapabilityError`
  variants are the extraction contract; do not invent a second error type or
  widen variant sets here.
- **Fail-closed cancellation.** `resolve_session_cancellation` never mints a
  token; an unregistered run refuses with
  `CapabilityError::CancellationUnavailable`.
