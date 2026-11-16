# Changelog

`nexus-workflow-ir` — the thin Workflow IR facade: the generated document contract plus Q2 structural validation.

Sections follow the workspace [CHANGELOG.md](../../CHANGELOG.md) format: `## [<version>] - <YYYY-MM-DD>`, groups **Features** / **Fixes** / **Docs & chores**, entries `- **scope**: description (abcdef0)`. This crate versions in the contract bracket of `tooling/release/version-brackets.json`, independently of the product version. A section becomes `## [<version>] - <YYYY-MM-DD>` when this crate is released under its own version; until then its notable changes sit under `[Unreleased]`. Dependency-bump commits are not listed.

## [Unreleased]

### Features
- **release**: carry the contract version bracket set (b6197d5)
- **workflow-ir**: walk resolved pattern fragments in the shared nest rules (15c7ef9)
- **pattern**: pin each fragment with the canonical content hash (13d2f15)
- **pattern**: read pattern package directories without hiding broken peers (9d9363b)
- **orchestration**: static simulate walk, trajectory and sample route (4413916)
- **workflow-ir**: add nexus-workflow-ir crate with Q2 structural validation (50d383c)

### Fixes
- **compose**: one resolved-pin definition, honest fragment rules, root-cause refusal (dc9586c)
- **compose**: validate fragments, honor parent declarations, refuse lost refs (1dc7a51)
- **workflow-ir**: reject depends_on targets missing from nodes (9145b00)

### Docs & chores
- apply the pinned nightly rustfmt to the simulation code (d357867)
