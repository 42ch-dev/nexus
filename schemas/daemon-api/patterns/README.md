# Daemon API — Pattern Package Schemas

JSON Schemas for the **pattern package registry** read-only endpoint `GET /v1/daemon/patterns`, plus the on-disk `pattern.toml` manifest the registry reads. These are cross-language contracts consumed by future WebApp/Web-UI clients.

v1.239 P3 — pattern packages (manifest, canonical content hash, filesystem registry).

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `GET /v1/daemon/patterns` | `list-patterns-query.schema.json`, `list-patterns-response.schema.json`, `pattern-summary.schema.json` |

## On-disk manifest

`pattern-manifest.schema.json` describes the enforced fields of one package's `pattern.toml`; `pattern-slot.schema.json` is the shared slot declaration. The manifest is data: it declares an id, a version label, an inclusive `ir_compat` range, a description, an effect class, slots and required capabilities. Whether a package is readable, IR-compatible and hash-consistent is decided at runtime by the registry reader — never by the manifest and never by registry presence.

## Related

- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Plan:** `v239-p3-pattern-packages`
