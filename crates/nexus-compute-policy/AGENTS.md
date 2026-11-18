# nexus-compute-policy — Private Sandbox Policy

The single owner of the compute sandbox's default limits and of how manifest
overrides resolve against them (compass Q6). Plain `std`, no dependencies, no
runtime: this crate decides *amounts*, never enforcement.

## Key Rules

- **Private, not SDK.** `publish = false`. It is not part of any public module
  SDK or wire schema, and no consumer may re-export it as a compatibility shim.
  `nexus-wasm-host` and `nexus-orchestration` depend on it directly.
- **Overrides tighten only.** `SandboxConfig::resolve` clamps every present
  override with `min(override, base)`. A module or descriptor can never raise
  the host's configured ceiling.
- **The base is the caller's configured value.** The runtime resolves against
  the `SandboxConfig` its engine was built with (including exact non-MiB byte
  and `Duration` caps), not the global default.
- **No enforcement here.** Fuel consumption, traps, `StoreLimits` and the epoch
  watchdog stay in `nexus-wasm-host`; descriptor validation and admission
  refusals stay in `nexus-orchestration`.

## Pre-release Note

Default values (`DEFAULT_FUEL`, `DEFAULT_MEMORY_MIB`, `DEFAULT_WALL_TIME`) are
policy, not wire contract: changing them requires no migration, but it changes
every invocation's ceiling at once.
