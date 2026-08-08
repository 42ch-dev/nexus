# Nexus Docs

Docs index. Integrator-facing guides first; maintainer docs below. The
[integrator walkthrough](../strategy-samples/README.md) is the end-to-end
companion to the three guides (runtime boot, Connect SDK, N-C1/N-C2 ops,
compute, fork + validate).

## Integrator docs

| Doc | Audience | Purpose |
|-----|----------|---------|
| [nexus-runtime.md](nexus-runtime.md) | Integrators | Install/run the headless `nexus-runtime` binary, Connect-only invoke surface, home layout, coexistence with the creator app, allowlist + `module_scope` setup. |
| [strategy-authoring.md](strategy-authoring.md) | Integrators | External strategy format (`preset.yaml` + `templates/`), trigger/scheduled lanes, prompt templates, validator, fork flow. |
| [module-authoring.md](module-authoring.md) | Integrators | WASM module ABI, marshalling, `manifest.json` (incl. `wasm_sha256`), `module_scope` allowlist, operator install, read-only compute. |

## Maintainer docs

| Doc | Audience | Purpose |
|-----|----------|---------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Maintainers | Directional map: product surfaces, hard boundaries, where authority lives. |
| [CODEGEN.md](CODEGEN.md) | Maintainers | Schema-first codegen workflow (JSON Schema → TypeScript + Rust). |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contributors | Setup, pre-PR checklist, local checks, documentation placement. |

## Normative sources

The ABI spec stays authoritative for the module contract: [`.mstar/specs/compute-module-abi.md`](../.mstar/specs/compute-module-abi.md).
