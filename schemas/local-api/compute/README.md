# Local API — Compute Module ABI Schemas

JSON Schemas for the **WASM compute module ABI envelopes** consumed by external compute modules (and, in future, the WebApp/Web-UI Local API client). These are cross-language contracts (Rust host ↔ `wasm32-unknown-unknown` module), so they live under `schemas/` and run through codegen rather than as hand-written local types.

V1.62 (2026-06-23) moved these from `schemas/compute/` into `schemas/local-api/compute/` (consumer-scope reorganization). The per-module entity shape schemas (`compute/entity-attributes`, `compute/entity-state`) were **deleted** in the same reorganization — per-module shapes now live in each module's `manifest.json` `schemas` block (V1.62 P1).

## Files (2)

| File | Role |
| --- | --- |
| `compute-input.schema.json` | `ComputeInput` envelope — KeyBlock snapshot + world ref + narrative state + module invocation params |
| `compute-output.schema.json` | `ComputeOutput` 4-part envelope — `state_delta`, `timeline_events`, `new_key_blocks`, `battle_report` |

## Related

- **Module authoring + `manifest.json` `schemas` block:** [modules/README.md](../../../modules/README.md)
- **Compute ABI normative spec:** `.mstar/knowledge/specs/compute-module-abi.md` (V1.62 P2 — placeholder until authored)
- **Runtime host:** `crates/nexus-wasm-host/` (re-exports `ComputeInput` / `ComputeOutput` from `nexus-contracts`)
- **Layout spec:** [schemas-directory-layout.md](../../../.mstar/knowledge/specs/schemas-directory-layout.md) §3.5

**Consumer:** `@42ch/nexus-contracts` (npm) + `nexus-wasm-host` Rust crate (re-exports the generated types) + external WASM compute modules.
