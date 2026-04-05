# Meta Schemas

This directory contains meta schemas defining structure and validation rules for all Nexus schemas.

## Files

- `meta.schema.json`: Defines required fields (`$schema`, `$id`, `schema_version`, `title`, `type`) for all Nexus schemas

## Schema Versioning

All Nexus schemas must include:
- `schema_version`: Integer (e.g., `1`)
- `$id`: URI following `https://nexus.42ch.io/schemas/<path>/<name>.schema.json`

Version bumps follow integer monotonic increment:
- **Breaking changes**: Increment to next integer
- **Backward-compatible additions**: Patch-level description updates only (same integer)
