# Changelog

`nexus-module-manifest` — the shared compute-module manifest contract: manifest types, the validation core and `wasm_sha256` injection. Standalone crate, `exclude`d from the workspace so members can path-depend on it.

Sections follow the workspace [CHANGELOG.md](../../CHANGELOG.md) format: `## [<version>] - <YYYY-MM-DD>`, groups **Features** / **Fixes** / **Docs & chores**, entries `- **scope**: description (abcdef0)`. This crate versions in the contract bracket of `tooling/release/version-brackets.json`, independently of the product version. A section becomes `## [<version>] - <YYYY-MM-DD>` when this crate is released under its own version; until then its notable changes sit under `[Unreleased]`. Dependency-bump commits are not listed.

## [Unreleased]

### Features
- **compute**: nexus42 compute CLI group + nexus-module-test mini-host (77544f0)
- **modules**: nexus-module-sdk + nexus-module-manifest extraction (4d30dd1)

### Fixes
- **modules**: P0 plan QC fix wave — validation/pairing contracts, drift-gate hardening, exit-code corrections (6eaebfb)
- **compute**: task-2 review round — validator, exit-code, and wire fixes (349ab4b)
