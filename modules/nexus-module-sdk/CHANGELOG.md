# Changelog

`nexus-module-sdk` — the official SDK for Nexus compute modules: ABI export shims, typed envelope, host-import wrappers and the manifest helper. Standalone crate, consumed by module crates from crates.io or a path override.

Sections follow the workspace [CHANGELOG.md](../../CHANGELOG.md) format: `## [<version>] - <YYYY-MM-DD>`, groups **Features** / **Fixes** / **Docs & chores**, entries `- **scope**: description (abcdef0)`. This crate versions in the contract bracket of `tooling/release/version-brackets.json`, independently of the product version. A section becomes `## [<version>] - <YYYY-MM-DD>` when this crate is released under its own version; until then its notable changes sit under `[Unreleased]`. Dependency-bump commits are not listed.

## [Unreleased]

### Features
- **modules**: basic-combat SDK migration + module-dx drift tooling (AR-7/11/12/13) (2e5659f)
- **modules**: nexus-module-sdk + nexus-module-manifest extraction (4d30dd1)

### Fixes
- **modules**: P0 plan QC fix wave — validation/pairing contracts, drift-gate hardening, exit-code corrections (6eaebfb)
- **compute**: task-2 review round — validator, exit-code, and wire fixes (349ab4b)
