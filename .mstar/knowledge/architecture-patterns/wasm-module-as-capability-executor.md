---
module: nexus-orchestration + nexus-daemon-runtime + nexus42 CLI + nexus-wasm-host
date: 2026-08-22
problem_type: architecture_pattern
category: architecture-patterns
severity: high
plan_id: 2026-08-22-v1.172-p1-wasm-capability-executor
applies_when:
  - "Extending the compile-time Capability registry with on-disk user capabilities"
  - "Authoring or reviewing a WASM-backed capability executor"
  - "Installing a capability.json + manifest.json + wasm trio under ~/.nexus42/capabilities/"
tags:
  - capability
  - wasm-executor
  - admission
  - sandbox-clamp
  - wasm-sha256
  - trio-install
  - origin
---

# WASM Module as Capability Executor (DR-10 minimal)

How a **compile-time** `Capability` registry is extended by on-disk user
capabilities (descriptor + manifest + wasm trio) executed through the existing
WASM sandbox. Distilled from V1.172 (AR-34..44). This is the *minimal*
extension path — not a plugin ABI.

## Context

The orchestration registry was closed to compile-time builtins. A developer
who wanted a new named capability had to fork the daemon and hand-copy ABI
glue. V1.172 reuses the shipped compute-module SDK + `nexus-wasm-host`
sandbox: a user capability is a WASM module plus a local descriptor, discovered
from `~/.nexus42/capabilities/<name>/` at **daemon restart**, invoked by name
on the existing preset `enter` / graph Task path.

## Guidance

### 1. Trio layout + raw-home helper

```text
~/.nexus42/capabilities/<name>/capability.json     # UserCapabilityDescriptor
~/.nexus42/capabilities/<name>/manifest.json       # module manifest
~/.nexus42/capabilities/<name>/<module-id>.wasm    # module-id = descriptor wasm.moduleId
```

Scan via `nexus_home_layout::user_capabilities_dir(raw_home)` — pass
`state.nexus_home().parent()`, never the nested `nexus_home` (double-nest
guard; see [nexus-home-layout-path-helpers.md](../conventions/nexus-home-layout-path-helpers.md)).
Missing dir → empty outcome, not an error. Per-entry parse/validation failure
→ `ScanOutcome.skipped` + `warn!`, never a top-level scan error, never a boot
failure.

### 2. Admission order is fail-closed (never brick, never half-register)

Per candidate, in this exact order
(`crates/nexus-orchestration/src/capability/admission.rs`):

1. **Collision** — name equals a builtin → skip (`NameCollision`). Builtin
   wins; no reserved prefix. Collision must be decided **before** append +
   `build_index`: the index is last-wins `HashMap` insert, so appending a
   colliding stub silently shadows the real builtin `run()` and duplicates
   the catalog row.
2. **Module file** — `manifest.json` + `<module-id>.wasm` both present.
3. **Hash** — descriptor `wasm.wasmSha256` equals the manifest-verified
   digest (`ModuleManifest::verify_wasm_sha256` — the **only** content-hash
   path). Missing declared hash → reject (fail-closed, no stat-fence here).
4. **Clamp** — sandbox overrides `min(override, host_default)` via
   `SandboxConfig::default()`. Clamp **never** rejects.

A skipped candidate is absent from both the registry and the catalog.

### 3. Carry clamped sandbox onto the executor — then min-clamp at run

Admission clamp is not enough. Store the clamped `SandboxOverrides` on
`UserCapability` and, at `run()`, fold each present field onto the module
manifest (`min(existing, descriptor)`). `WasmEngine::resolve_sandbox` then
does `min(manifest_override, host_default)`.

Validating overrides at admit and dropping them produces a **silent no-op**:
the author writes `sandbox: { fuel: 1_000_000 }` and the invocation still
runs at host/manifest defaults. Re-clamp in `UserCapability::new` so a
direct constructor cannot carry un-clamped bounds. Do not change
`sandbox.rs`.

### 4. Re-verify `wasm_sha256` on every lazy load (TOCTOU)

Admission hashes once. `run()` reads the dir lazily and
`ModuleCache::get_or_compile` is keyed by bytes-hash — edited-after-admit
bytes would compile. Before compile, re-run `verify_wasm_sha256` and require
`manifest.wasm_sha256 == admitted descriptor hash`. Mismatch →
`InputInvalid("module '<id>' hash changed for capability '<name>'")`.

### 5. CLI install: identity cross-check + atomic trio

`nexus42 capability validate|list|install` (`#[command(hide = true)]`; no
`run` / `scaffold`). `verify_pairing` must assert
`descriptor.wasm.moduleId == manifest.module_id` **and** the sha256 pair.
A module-id mismatch otherwise installs with exit 0 and is skipped at boot
(missing `<manifestModuleId>.wasm`) — a silent dead install.

Copy via sibling staging + two same-filesystem renames (current → backup,
staging → dir). `<name>/` is always an old-complete or new-complete trio,
never a partial one. Existing dir is overwritten after re-verify.

### 6. Catalog honesty + wire posture

`Capability::origin()` defaults to `Builtin` (zero edits to 34 builtins);
user override → `User`. Handler maps the enum to a **string** on the local
DTO — the orchestration enum must not enter `nexus-contracts` (dependency
direction).

The daemon serves the **local** `CapabilityInfo`
(`#[serde(rename_all = "camelCase")]`): `inputSchema` / `outputSchema` /
`origin`. Generated schema/TS names are snake_case. Page + tests must match
the handler's serialized shape.

Additive `origin` is a **sanctioned-diff** wire change, not
`wire_contracts_changed: false`. See
[wire-contracts-frozen-verification.md](../conventions/wire-contracts-frozen-verification.md).

### 7. Lifetime + engine-absent

Leak `name` / `input_schema` / `output_schema` once per admitted capability
per boot (`Box::leak`) so the `&'static str` trait stays unchanged. Do not
add a `CapabilityError` variant — engine-absent `run()` uses existing
`WorkerUnavailable`.

## Why This Matters

- Last-wins index insert + late collision check = **silent builtin shadow**.
- Clamp-at-admit-then-drop = **author-visible sandbox that does nothing**.
- Admit-only hash = **TOCTOU**: post-restart dir edits execute unverified
  bytes through a hash-keyed cache.
- Pairing without module-id equality = **exit-0 dead install**.
- Partial trio copy = a half-registered capability on the next boot.

## When to Apply

- Adding or reviewing user-authored capabilities (descriptor, scan, admit,
  executor, CLI install, catalog provenance).
- Any new on-disk artifact that registers into a last-wins name index.
- Any "override recorded at gate, enforced at run" pair (carry the value).

## Examples

### Before — collision after append (shadow)

```rust
self.capabilities.append(&mut admitted); // user "sync.pull" last
self.build_index();                     // HashMap last-wins → user stub
```

### After — skip inside the scan against the builtin name set

```rust
let builtin_names: HashSet<&str> = self.capabilities.iter().map(|c| c.name()).collect();
let mut outcome = scan_user_capabilities(dir, &builtin_names, engine, cache);
self.capabilities.append(&mut outcome.admitted);
self.build_index();
```

### Before — sandbox validated, then dropped

```rust
// admit() clamps, then constructs UserCapability { dir, module_id, engine, cache }
// run() → resolve_sandbox(manifest only) → descriptor fuel ignored
```

### After — carry + fold at run

```rust
if let Some(fuel) = sandbox.fuel {
    manifest.max_fuel = Some(manifest.max_fuel.map_or(fuel, |m| m.min(fuel)));
}
engine.compute(&module, &manifest, &compute_input)?;
```

## References

- Module authoring (the wasm half): [../engineering/compute-module-sdk-authoring-pattern.md](../engineering/compute-module-sdk-authoring-pattern.md)
- Embedded-pin hash gotcha: [../best-practices/embedded-pinned-wasm-sha256-alignment.md](../best-practices/embedded-pinned-wasm-sha256-alignment.md)
- Raw-home helpers: [../conventions/nexus-home-layout-path-helpers.md](../conventions/nexus-home-layout-path-helpers.md)
- Wire gate (AR-40 sanctioned diffs): [../conventions/wire-contracts-frozen-verification.md](../conventions/wire-contracts-frozen-verification.md)
