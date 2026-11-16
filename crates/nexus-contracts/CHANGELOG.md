# Changelog

`nexus-contracts` — the generated Rust wire types produced from `schemas/`; JSON Schema is the truth source and `pnpm run codegen` rewrites `src/generated/`.

Sections follow the workspace [CHANGELOG.md](../../CHANGELOG.md) format: `## [<version>] - <YYYY-MM-DD>`, groups **Features** / **Fixes** / **Docs & chores**, entries `- **scope**: description (abcdef0)`. This crate versions in the contract bracket of `tooling/release/version-brackets.json`, independently of the product version. A section becomes `## [<version>] - <YYYY-MM-DD>` when this crate is released under its own version; until then its notable changes sit under `[Unreleased]`. Dependency-bump commits are not listed.

## [Unreleased]

### Features
- **contracts**: add pattern package manifest and listing schemas (12184ac)
- **contracts**: generate trajectory and simulation request types (c63bd4f)
- **workflow-ir**: add v0 WorkflowDocument schema and generate contracts (53ec895)
- **contracts**: rule archive status descriptions + listWorldRules includeArchived (V1.198 P0 T1) (c997072)
- **contracts**: add P5-T0 core schema and generated contract lane (988ae8d)
- **contracts**: rename Local API surface to Daemon API and add remote-bind gate (V1.90) (4fb97e2)
- **codegen**: generate TypeScript and Rust types from 15 JSON schemas (6c31094)
- **contracts**: codegen (eb01949)
- initialize Rust workspace with nexus-contracts placeholder (71932a7)

### Fixes
- **contracts**: constrain pattern manifest version and slot fields (e75f42b)
- **contracts**: require the nullable Timeline overview cursor (436bce5)
- **contracts**: tri-state marker annotation in ts-gen + strict drift registry for core schemas (750c99c)
- **contracts**: encode P5-T0 review invariants in core schemas (c52fb3e)
- **contracts**: resolve same-file $ref definitions in drift dummy synthesis (c98be6c)
- **deps**: drop three unused dependency declarations (7785570)
