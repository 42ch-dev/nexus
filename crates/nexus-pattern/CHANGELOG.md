# Changelog

`nexus-pattern` — the read-only pattern package registry: strict `pattern.toml` manifests plus the `workflow.ir.json` fragment they carry.

Sections follow the workspace [CHANGELOG.md](../../CHANGELOG.md) format: `## [<version>] - <YYYY-MM-DD>`, groups **Features** / **Fixes** / **Docs & chores**, entries `- **scope**: description (abcdef0)`. This crate versions in the contract bracket of `tooling/release/version-brackets.json`, independently of the product version. A section becomes `## [<version>] - <YYYY-MM-DD>` when this crate is released under its own version; until then its notable changes sit under `[Unreleased]`. Dependency-bump commits are not listed.

## [Unreleased]

### Features
- **release**: public contract-bracket entry with an independent literal version and caret registry requirements (v249-public-packages K1)
