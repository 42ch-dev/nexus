# compute module sdk authoring pattern — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/compute-module-sdk-authoring-pattern.md`](../../../knowledge/architecture-patterns/compute-module-sdk-authoring-pattern.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 28

```markdown
How the official WASM module SDK (`nexus-module-sdk` + `nexus-module-manifest` + `nexus-module-test`, V1.170 P0) is designed so third-party module authors write zero ABI code and the SDK drifts neither from the wire nor from the host. The normative ABI contract lives in `.mstar/specs/active/compute/compute-module-abi.md` — this doc captures the **authoring pattern**, not the wire spec.
```

### at pre-freeze line 32

```markdown
Before V1.170, the only compute module (`modules/basic-combat`) hand-wrote the ABI exports (`alloc`/`init`/`compute` with `#[no_mangle]`), the global allocator, the manifest structs, and the accessor helpers — every line a third-party author would have to copy. The pivot-D goal: an official SDK where module authors write a single `fn` and one macro call, with DR-49 (ABI V2) able to land additively, and a machine-checkable guarantee that SDK mirror types track the wire.
```

### at pre-freeze line 77

```markdown
### 4. Wire-required vs lock-assumed (the WorldRef lesson)
```

### at pre-freeze line 79

```markdown
The lock spec initially wrote `world_ref` as a struct of strict `String` fields. The real wire says otherwise: `compute-input.schema.json` requires the `world_ref` **object** but **none of its properties**, and the host's generated `ComputeInputWorldRef` is all-`Option` — the `killing_blow_marks_defender_not_alive` fixture sends `{"world_id": …}` only. Strict `String` fields would deserialize-fail (`InputMalformed`, `-1`) where the real host succeeds, breaking byte-compat.
```

### at pre-freeze line 81

```markdown
**Rule:** validate SDK mirror types against the host's **generated** types and the **real-host fixtures**, not just the schema prose — a schema-level `required` array on an object says nothing about its properties' optionality. Lock-assumed strictness the wire does not enforce is a byte-compat bug waiting to happen. (Resolution: `Option<String>` + `#[serde(default)]` on all three fields — AR-3 amendment.)
```

### at pre-freeze line 85

```markdown
The mirror strategy is hand-maintained types + a drift guard (typify-codegen-published and `nexus-contracts`-features variants were rejected: typify shape leakage already cost a consumer fix wave in V1.138; publishing `nexus-contracts` turns an internal crate into a public semver surface). The codegen path stays the **named graduation path** when modules > ~5 or DR-49 grows the envelope — the SDK keeps a clean `types/` module for codegen to overwrite.
```

### at pre-freeze line 95

```markdown
`ModuleManifest::validate()` exists **twice** (SDK + `nexus-module-manifest`). The V1.170 P0 fix-wave lesson (QC2 review of the v1.170 P0 computable-DX spine): `compute validate` ran only `manifest.validate()` while `build`/`install` also ran the path-safety guard (`validate_run_id_safe` on `module_id`) — a manifest with `module_id: "../evil"` reported `valid: true`. **Rule:** every validation rule must live at (a) the CLI command guard, (b) `ModuleManifest::validate()` in `nexus-module-manifest`, and (c) the SDK mirror — defense in depth, aligned by a shared test corpus. Sandbox default constants (`DEFAULT_FUEL`/`DEFAULT_MEMORY_MIB`/`DEFAULT_WALL_TIME_MS`) are public SDK constants so manifest generators and validators agree with the host by construction.
```

### at pre-freeze line 163

```markdown
- Iteration spec: v1.170 (`v1.170-computable-dx-locks.md`) AR-2..AR-12
```
