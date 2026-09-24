# Daemon API — Compute Schemas

The two ABI envelopes describe the Rust host ↔ WASM module exchange. The remaining schemas describe generated module-discovery and run/review/history DTOs shared by the native service and Web client. All are generated cross-language contracts, not hand-written local types.

V1.62 (2026-06-23) moved these from `schemas/compute/` into `schemas/daemon-api/compute/` (consumer-scope reorganization). The per-module entity shape schemas (`compute/entity-attributes`, `compute/entity-state`) were **deleted** in the same reorganization — per-module shapes now live in each module's `manifest.json` `schemas` block (V1.62 P1).

## Contracts

| Group | Files | Role |
| --- | --- | --- |
| Module ABI | `compute-input.schema.json`, `compute-output.schema.json` | Immutable input snapshot and four-part proposal output |
| Module discovery | `module-summary`, `module-detail`, `list-modules-response` | Installed module listing and invocation schema |
| Run and review | `run-*`, `list-runs-query`, `clear-runs-*`, `discard-run-response` | Invocation, detail/history, accept/discard and world-scoped terminal clear |

File names in the latter two groups carry the `.schema.json` suffix. The ABI is specified independently of the HTTP run lifecycle.

## Related

- **Module authoring + `manifest.json` `schemas` block:** [modules/README.md](../../../modules/README.md)
- **Compute ABI normative spec:** [compute-module-abi.md](../../../.mstar/specs/compute-module-abi.md)
- **Runtime host:** `crates/nexus-wasm-host/` (re-exports `ComputeInput` / `ComputeOutput` from `nexus-contracts`)
- **Layout spec:** [schemas-directory-layout.md](../../../.mstar/specs/schemas-directory-layout.md) §3.5

**Consumers:** `@42ch/nexus-contracts` (npm), `nexus-wasm-host` Rust crate (ABI envelope re-exports), the native service and Web client (run/discovery DTOs), and external WASM compute modules (ABI envelopes).
