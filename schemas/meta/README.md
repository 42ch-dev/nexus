# Meta Schemas

This directory contains meta schemas defining structure and validation rules for all Nexus schemas.

## Files

- `meta.schema.json`: Defines required fields (`$schema`, `$id`, `schema_version`, `title`, `type`) for all Nexus schemas

## Schema URIs and `{NEXUS42_BASE_URL}`

**In documentation** (READMEs, runbooks, API guides), refer to the eventual HTTPS **origin** as `**{NEXUS42_BASE_URL}`** with **no trailing slash**. Full schema identifiers are then:

`{NEXUS42_BASE_URL}/schemas/<path>/<name>.schema.json`

**In committed JSON Schema files**, `$id` and `$ref` must be valid URIs for tooling (AJV `format: uri`, CI). This repository therefore uses the reserved host `**https://nexus42.invalid`** (see [RFC 6761](https://datatracker.ietf.org/doc/html/rfc6761) — `.invalid` is guaranteed non-resolvable). Example:

`https://nexus42.invalid/schemas/common/common.schema.json`

When a product domain is chosen, you can publish the same path layout under `{NEXUS42_BASE_URL}` and optionally migrate `$id` / `$ref` in a coordinated release (or keep `nexus42.invalid` as a stable logical namespace if your toolchain allows).

## Schema Versioning

All Nexus schemas must include:

- `schema_version`: Integer (e.g., `1`)
- `$id`: URI as above (`https://nexus42.invalid/schemas/...` in this repo)

Version bumps follow integer monotonic increment:

- **Breaking changes**: Increment to next integer
- **Backward-compatible additions**: Patch-level description updates only (same integer)