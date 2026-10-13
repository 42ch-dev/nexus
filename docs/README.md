# Nexus Docs

Docs index. Integrator-facing guides first; maintainer docs below. The
[integrator walkthrough](../strategy-samples/README.md) is the end-to-end
companion to the three guides (runtime boot, Connect SDK, N-C1/N-C2 ops,
compute, fork + validate).

## Integrator docs

| Doc | Audience | Purpose |
|-----|----------|---------|
| [nexus-runtime.md](nexus-runtime.md) | Integrators | Install/run the headless `nexus-runtime` binary, Connect-only invoke surface (core ops + Connect `tools.*`), home layout, coexistence with the creator app, allowlist + `module_scope` setup. |
| [strategy-authoring.md](strategy-authoring.md) | Integrators | External strategy format (`preset.yaml` + `templates/`), trigger/scheduled lanes, prompt templates, validator, fork flow. |
| [module-authoring.md](module-authoring.md) | Integrators | WASM module ABI, marshalling, `manifest.json` (incl. `wasm_sha256`), `module_scope` allowlist, operator install, read-only compute. |
| [mcp-server.md](mcp-server.md) | Operators | Retained MCP/peer-tool contract: exact-tool-id admission (both sides name the same id), allowlist-edit restart semantics, one catalog, draft-2020-12 builtin schemas, visibility policy, peer-control lane. The removed Model A `nexus42 mcp serve` bridge record is archived under `.mstar/specs/archived/architecture/`. |

## Maintainer docs

| Doc | Audience | Purpose |
|-----|----------|---------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Maintainers | Directional map: product surfaces, hard boundaries, where authority lives. |
| [CODEGEN.md](CODEGEN.md) | Maintainers | Schema-first codegen workflow (JSON Schema → TypeScript + Rust). |
| [release.md](release.md) | Maintainers | Release governance: dispatch inputs, pipeline stages, prerelease gate, local dry-runs, recovery, signing arrival. |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contributors | Setup, pre-PR checklist, local checks, documentation placement. |

## Normative sources

The ABI spec stays authoritative for the module contract: [`.mstar/specs/active/compute/compute-module-abi.md`](../.mstar/specs/active/compute/compute-module-abi.md).
