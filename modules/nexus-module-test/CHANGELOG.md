# Changelog

`nexus-module-test` — the ABI-conformance mini-host for Nexus compute modules: a wasmtime-based harness that runs a compiled `.wasm` against the SDK envelope. Standalone crate, used as a host-target dev-dependency.

Sections follow the workspace [CHANGELOG.md](../../CHANGELOG.md) format: `## [<version>] - <YYYY-MM-DD>`, groups **Features** / **Fixes** / **Docs & chores**, entries `- **scope**: description (abcdef0)`. This crate versions in the contract bracket of `tooling/release/version-brackets.json`, independently of the product version. A section becomes `## [<version>] - <YYYY-MM-DD>` when this crate is released under its own version; until then its notable changes sit under `[Unreleased]`. Dependency-bump commits are not listed.

## [Unreleased]

### Features
- **release**: prove the contract bracket is packageable offline (3897550)
- **modules**: basic-combat SDK migration + module-dx drift tooling (AR-7/11/12/13) (2e5659f)
- **compute**: nexus42 compute CLI group + nexus-module-test mini-host (77544f0)

### Fixes
- **v1.198-p2-t6**: keep wasm module admission at the v48 set under wasmtime 49 (ef5c22b)
- fence control snapshots and make schedule deletion atomic (e67ab04)
- complete dependency sweep integration checks (c0decd9)
- **modules**: P0 plan QC fix wave — validation/pairing contracts, drift-gate hardening, exit-code corrections (6eaebfb)
- **compute**: task-2 review round — validator, exit-code, and wire fixes (349ab4b)
