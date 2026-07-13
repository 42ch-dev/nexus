# Compute Manifest Bridge Reconciliation (V1.115 P2)

> Iteration-scoped product/tech brief for V1.115 P2. Not a normative `{SPECS_DIR}`
> Master — reconciles compute foundation type honesty so a future module
> authoring SDK has a single verified target.

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-13-v1.115-compute-manifest-reconciliation` |
| **Tier** | Must |
| **Audience** | Maintainers + future module authors (Modules panel behavior unchanged for end authors) |

## Problem framing

V1.114 shipped the compute module registry (`list_modules` / `get_module`) with a
deliberate **JSON round-trip bridge** from hand-written `ModuleManifest` to
generated `ModuleDetail` (`R-V1114P2QC1-W002`). It works today because the shapes
happen to match, but drift fails at **runtime** (or silently drops fields)
instead of at **compile / schema gate** time.

That is the wrong failure mode for a foundation the next compute work will sit
on (more modules, authoring SDK, canvas↔compute intersection).

## User value

| Who | Why they care |
| --- | --- |
| **Authors (Modules panel)** | No visible change — same module list/detail content as V1.114 |
| **Module / SDK authors (future)** | Manifest shape they target is the generated wire contract, not an unverified twin |
| **Maintainers** | Drift fails the build / schema gate; no silent JSON-bridge masking |

Product story for this plan is **trust in the compute foundation**, not a new
author-facing feature.

## Goals

1. Eliminate `serde_json::to_value` → `from_value` in `manifest_to_detail()`.
2. Typed conversion (`From<&ModuleManifest> for ModuleDetail` — non-lossy per
   architect field audit; all 15 fields shared) from runtime manifest → wire
   `ModuleDetail`.
3. Bidirectional `schema_drift_detection` (or equivalent) proven to fail on
   deliberate drift — schema↔generated via existing gate; manifest↔generated
   via compile-time `From`.
4. Keep embedded-module loader and `basic-combat` behavior identical.
5. Clarify wire vs runtime-only fields in `compute-module-abi.md` §7 —
   architect pre-audit confirms all fields are wire today (no runtime-only
   split exists).

## Non-goals

- New compute modules
- Module authoring SDK / tooling UI
- Compute state editor (write `body.state`)
- Multi-module composition / CDN distribution / marketplace
- Breaking wire changes or `@42ch/nexus-contracts` major bump
- Reversing reconciliation (hand-written becomes truth over generated)

## Target state

- Generated `ModuleDetail` is the wire SSOT; hand-written `ModuleManifest`
  holds runtime-only fields + converts via typed path.
- Shared-field drift is a compile-time or gate failure.
- Modules panel outputs match V1.114 for `basic-combat`.

## Acceptance criteria (maintainer-observable; author-facing = no regression)

| ID | Criterion | How to verify |
| --- | --- | --- |
| **AC-P2-1** | `manifest_to_detail()` uses typed conversion — no JSON round-trip | Code review of `registry.rs` |
| **AC-P2-2** | Shared-field drift between manifest and generated `ModuleDetail` fails build or schema gate (not silent runtime drop) | Deliberate-drift test or demonstrated gate; document which mechanism |
| **AC-P2-3** | `schema_drift_detection` (or repo equivalent covering compute manifest) passes bidirectionally | Existing + extended tests green |
| **AC-P2-4** | `list_modules()` / `get_module()` results for `basic-combat` match V1.114 on shared fields | Regression tests; Modules panel smoke unchanged |
| **AC-P2-5** | Runtime-only fields (none today per architect audit — all 15 fields are wire-promoted) remain on `ModuleManifest` and are not forced onto wire; classification recorded after T1 | Field classification table in plan/spec after T1 audit |

## Product decisions (locked this seat)

| Decision | Choice | Rationale |
| --- | --- | --- |
| Reconciliation direction | **Generated `ModuleDetail` is the target** | Wire contracts are truth (STRATEGY principle #2) |
| New wire fields this iteration | **None by default** | Foundation honesty, not feature expansion. If T1 audit finds a field that is already exposed to clients but missing from generated schema, promote only that field (additive) — still not a new product feature |
| Runtime-only fields | All 15 manifest fields are wire today (architect pre-audit). No runtime-only split exists. Forward-looking: a future runtime-only field stays on `ModuleManifest`, omitted from the `From` impl | `compute_export` / `init_export` are wire-promoted (in schema), not runtime-only. The "runtime-only" framing in earlier drafts was incorrect. |
| Capacity cut | Prefer proving the drift gate (T3) over doc polish (T5) if forced | Gate is the product promise |

## Plan independence note

P2 touches `nexus-wasm-host` / compute schemas only — **fully parallel** to P0/P1
canvas work. No product dependency.

## Architect decisions (Seat 2 — resolved)

| # | Question | Decision | Rationale |
| --- | --- | --- | --- |
| 1 | Field audit: any wire-silent field that must be promoted? | **No.** All 15 `ModuleManifest` fields are already wire-promoted (present in `module-detail.schema.json` and generated `ModuleDetail`). No field is missing from either side. | Verified against `manifest.rs` field list and `module-detail.schema.json` properties — exact 1:1 match. `additionalProperties: false` on the schema confirms no hidden fields. |
| 2 | `From` vs `TryFrom` for conversion? | **`From` (non-lossy).** | All fields are shared; the conversion is a 1:1 field copy that cannot fail. `TryFrom` would imply fallibility that does not exist. Caveat: verify the `host_functions` enum representation matches between hand-written `HostFunction` and the generated enum (both `snake_case`). |
| 3 | Does `schema_drift_detection` already cover `compute/manifest.json`? | **Partially.** The gate covers schema ↔ generated `ModuleDetail`/`ModuleSummary` (`Strict`, lines 198–213). It does NOT cover hand-written `ModuleManifest`. After T2, manifest ↔ generated drift is enforced at **compile time** by the `From` impl — stronger than any runtime gate. | The JSON round-trip masked drift because it silently dropped mismatched fields. The typed `From` turns drift into a compile error. T3's deliberate-drift test proves this. |
| 4 | Should `manifest_to_summary` share helpers with `manifest_to_detail`? | **No shared helper (YAGNI).** | `manifest_to_summary` is already field-by-field typed. `ModuleSummary` is a strict subset + runtime `status`. A shared helper adds abstraction with no benefit. Detail forgetfulness → compile error (`From`); summary forgetfulness → AC-P2-4 regression test. A cross-reference comment suffices. |

### Field classification table (T1 pre-audit — architect)

| Field | `ModuleManifest` | `module-detail.schema.json` | Generated `ModuleDetail` | Classification |
| --- | :-: | :-: | :-: | --- |
| `module_id` | ✓ | ✓ | ✓ | shared (wire) |
| `name` | ✓ | ✓ | ✓ | shared (wire) |
| `version` | ✓ | ✓ | ✓ | shared (wire) |
| `nexus_abi_version` | ✓ | ✓ | ✓ | shared (wire) |
| `required_key_block_types` | ✓ | ✓ | ✓ | shared (wire) |
| `compute_export` | ✓ | ✓ | ✓ | shared (wire) |
| `init_export` | ✓ | ✓ | ✓ | shared (wire) |
| `description` | ✓ (opt) | ✓ | ✓ | shared (wire) |
| `author` | ✓ (opt) | ✓ | ✓ | shared (wire) |
| `host_functions` | ✓ | ✓ (enum) | ✓ | shared (wire) — verify enum repr |
| `schemas` | ✓ (opt) | ✓ | ✓ | shared (wire) |
| `battle_report_kind` | ✓ (opt) | ✓ | ✓ | shared (wire) |
| `max_fuel` | ✓ (opt) | ✓ | ✓ | shared (wire) |
| `max_memory_mib` | ✓ (opt) | ✓ | ✓ | shared (wire) |
| `max_wall_time_ms` | ✓ (opt) | ✓ | ✓ | shared (wire) |

**Result:** all 15 fields shared. **Runtime-only fields today: none.**
`ModuleSummary` adds a runtime `status` (`"ok"`/`"broken"`) not on the
manifest — derived at registry call time, not stored.

## Spec refs

- `crates/nexus-wasm-host/AGENTS.md`
- `.mstar/specs/compute-module-abi.md` §7
- `schemas/daemon-api/compute/`
- Residual: `R-V1114P2QC1-W002`
