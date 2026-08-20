---
module: Cargo workspace + modules/
date: 2026-08-21
problem_type: convention
category: engineering
severity: medium
plan_id: 2026-08-20-v1.170-p0-computable-dx-spine
applies_when: [adding a publishable non-workspace crate to the repo, converting a workspace crate to standalone, path-depending on a crate from a workspace member, reviewing Cargo.toml topology changes]
tags: [cargo-workspace, standalone-crate, path-dependency, crate-topology, publish, modules, workspace-exclude]
---

# Standalone Crate Topology in a Cargo Workspace

How the nexus repo hosts crates.io-publishable crates **inside** the repo without making them workspace members: the empty-`[workspace]` mechanism, when the root `exclude` entry is needed, and how consumers resolve them.

## Context

V1.170 P0 added three publishable standalone crates under `modules/` (`nexus-module-sdk`, `nexus-module-manifest`, `nexus-module-test`) — the one non-workspace root the repo already had (`modules/basic-combat` precedent). They are in-repo (D4 lock) but must stay crates.io-publishable: independent versioning, no `version.workspace = true`, and no workspace-member coupling. Cargo's workspace resolution makes this topology fiddly in exactly one place: a path dependency **from a workspace member onto** such a crate.

## Guidance

### 1. Every standalone crate gets an empty `[workspace]` table at its manifest tail

```toml
[workspace]
```

This makes the crate its own workspace root: `cargo build`/`cargo test` inside it never traverse up into the nexus workspace, and its lockfile/versioning are fully independent. All standalone crates get it, no exceptions (precedent: `modules/basic-combat/Cargo.toml`).

### 2. Root `exclude` only for crates that workspace members path-depend on

The root `Cargo.toml` carries a single added line:

```toml
[workspace]
exclude = ["modules/nexus-module-manifest"]
```

**The conflict mechanics:** a path dependency from a workspace member onto a crate that declares its own `[workspace]` otherwise conflicts with Cargo's workspace resolution — the dep would make the crate both inside the workspace (via the member) and outside it (via its own root). The `exclude` entry tells Cargo to treat the crate as external to the workspace; the path dep then resolves legally.

**Which crates need the entry — the decision matrix:**

| Condition | Needs root `exclude`? |
| --- | --- |
| Workspace members path-depend on it (`nexus-module-manifest` ← `nexus-wasm-host` `[dependencies]` + `[build-dependencies]`, `apps/nexus42`) | **Yes** |
| Only other non-workspace crates depend on it (`nexus-module-sdk` consumed by module crates; `nexus-module-test` dev-dep of module crates) | **No** — a path dep between two non-workspace crates cannot join the root workspace |
| Nothing in the workspace touches it | No |

`exclude` is not a per-crate opt-out ritual; it is load-bearing only where a workspace member pulls the crate in.

### 3. Never put them in the root `members` list

No standalone crate is a workspace member. Consequences by construction: independent `version = "0.1.0"` in each manifest (`version.workspace = true` is impossible), and workspace-level pin inheritance is unavailable — standalone manifests carry literal pins that must be kept aligned with workspace pins manually (`sha2 = "0.10"` matching the workspace pin; `wasmtime = "46"` matching `nexus-wasm-host`).

### 4. Consumption: crates.io form vs path form

- Published releases consume the crates.io form: `nexus-module-sdk = "0.1"`.
- In-repo development may use the path override: `{ path = "../nexus-module-sdk" }`.
- Same crate, two valid dependency forms — the crates.io form is the distribution contract; the path form is a local-dev convenience.

### 5. `[lib]` crate type discipline

SDK-style crates compile for host targets (their own `cargo test` runs in CI) **and** `wasm32-unknown-unknown` (consumed by module cdylibs) — keep the default rlib crate type. Only final module crates declare `crate-type = ["cdylib"]`.

## Why This Matters

- The exclude decision matrix is non-obvious: an empty `[workspace]` alone is insufficient exactly where a workspace member path-depends on the crate, and the resulting failure is an opaque workspace-resolution error at build/metadata time, not a helpful message.
- Over-applying `exclude` is harmless but noisy; under-applying it breaks the workspace. Knowing *why* the entry exists (the member-path-dep conflict) lets a reviewer check the right condition instead of copying config.
- In-repo standalone is the only shape that is simultaneously publishable and CI-covered by the repo's existing legs (`cargo test` inside each crate runs without workspace coupling).

## When to Apply

- Adding any new publishable standalone crate (future module crates, SDK evolution under DR-49).
- Adding a path dependency from a workspace member onto a `modules/` crate — check the exclude entry first.
- Reviewing `Cargo.toml` diffs that touch `members`/`exclude` or add `[workspace]` tables.

## Examples

### V1.170 (three crates)

```toml
# modules/nexus-module-sdk/Cargo.toml — no root exclude needed (only module crates consume it)
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[target.'cfg(target_arch = "wasm32")'.dependencies]
dlmalloc = { version = "0.2", features = ["global"] }

[workspace]   # tail — own workspace root

# modules/nexus-module-manifest/Cargo.toml — root exclude REQUIRED (workspace members path-depend)
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"

[workspace]   # tail

# Root Cargo.toml — the single added line
[workspace]
exclude = ["modules/nexus-module-manifest"]
```

## References

- Normative ABI: `.mstar/specs/compute-module-abi.md`
- Iteration spec: `.mstar/iterations/v1.170/specs/v1.170-computable-dx-locks.md` AR-1
- Workspace dependency hygiene (members only): [../crate-selection-best-practices.md](../crate-selection-best-practices.md)
