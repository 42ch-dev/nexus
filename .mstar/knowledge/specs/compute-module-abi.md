# Nexus Compute Module ABI (V1 Envelope)

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Normative — V1.62 Shipped |
| **Document class** | Master |
| **Scope** | V1 envelope ABI: `ComputeInput` / `ComputeOutput` wire contracts, module exports table, host import ABI, marshalling convention, `manifest.json` contract, sandbox cross-ref, versioning policy |
| **Last updated** | 2026-06-23 — V1.62 P2 |
| **Related** | [wasm-host.md](./wasm-host.md), [schemas-directory-layout.md](./schemas-directory-layout.md) §3.5, [orchestration-engine.md](./orchestration-engine.md) §8 (narrative.compute), [entity-scope-model.md](./entity-scope-model.md) §5.5.9 |

This Master is normative for the V1 compute ABI — the interface contract between the
`nexus-wasm-host` runtime and a WASM compute module. It consolidates the V1.61
compass decisions (grill Q3/Q6/Q8) and the module authoring guide
(`modules/README.md`) into a single durable reference. Module authors should read
this document alongside `modules/README.md`; host implementors should read this
document alongside `wasm-host.md`.

---

## 1. V1 envelope ABI overview

A compute module is a **stateless pure function**. Each invocation receives a
`ComputeInput` envelope from the host and returns a `ComputeOutput` envelope.
The host runs the module inside a **per-invocation sandbox** — a fresh wasmtime
`Store` + `Instance` is created for every `compute()` call, so no state
carries over between invocations (compass V1.61 Q6, V1.62 Q5, V1.62 Q6).

```text
┌─────────────────┐     ComputeInput      ┌──────────────┐
│  nexus-wasm-host │ ──────────────────▶  │  WASM module │
│  (runtime)       │                      │  (stateless) │
│                  │ ◀──────────────────  │              │
└─────────────────┘     ComputeOutput     └──────────────┘
```

The module targets `wasm32-unknown-unknown` (no WASI required). The host places
the input JSON and output buffer inside the module's linear memory; the module
reads input, computes, and writes output — all through pointers into its own
memory. No filesystem, no networking, no wall clock are available inside the
sandbox.

This V1 envelope is the **only** ABI for V1.x. V2 may introduce multi-module
composition, a richer host-import surface, and CDN distribution
(see §9 Versioning).

---

## 2. Module exports table

Every V1 compute module must export `memory` and `alloc`. It must export
`compute` with the signature below. The `init` export is optional.

| Export | Signature | Required | Purpose |
| --- | --- | --- | --- |
| `memory` | exported linear memory | **yes** | The host reads input JSON and writes output JSON into this memory. |
| `alloc` | `(len: u32) -> u32` | **yes** | Allocate `len` bytes in linear memory; return a pointer. The host calls this to reserve space for input JSON (before `compute`) and the output buffer. |
| `compute` | `(in_ptr: u32, in_len: u32, out_ptr: u32, out_cap: u32) -> i64` | **yes** | Read `ComputeInput` JSON from `[in_ptr, in_ptr+in_len)`, compute, write `ComputeOutput` JSON to `[out_ptr, out_ptr+written)`, return `written` as `i64`. Negative return values are error sentinels (see §6). |
| `init` | `() -> ()` | **no** | One-shot setup called once after instantiation, before the first `compute()` call. Omitted if no setup is needed. |

The `compute` export name is configurable — the module declares the actual export
name in `manifest.json` (`compute_export` field). The signatures above are
fixed; the name may vary.

### 2.1 `init` semantics

If the module exports `init` and the manifest declares `init_export` to a
non-empty string, the host calls `init()` exactly once after instantiation,
before any `compute()` call. The `init` function takes no arguments and returns
nothing. It is intended for one-shot module initialization (e.g., seeding a
deterministic RNG, pre-allocating data structures). If `init` traps, the
invocation fails immediately with a `ComputeError::Trap` — the host does not
retry.

---

## 3. Host import ABI table

A module may import up to two host functions from the `nexus` module namespace.
The host registers only the functions the module's `manifest.json` whitelists
(see §7 `host_functions`). Importing a non-whitelisted function fails
instantiation.

| Import | Signature | Behavior |
| --- | --- | --- |
| `nexus::kb_read` | `(id_ptr: u32, id_len: u32, out_ptr: u32, out_cap: u32) -> i64` | Look up a `KeyBlock` by ID in the invocation's `key_blocks` snapshot; write its JSON to `out`. Returns bytes written (`>= 0`), `-1` if not found, `-2` if `out_cap` too small. |
| `nexus::narrative_query` | `(q_ptr: u32, q_len: u32, out_ptr: u32, out_cap: u32) -> i64` | Return narrative context JSON. V1 passes through the `narrative_state` from the `ComputeInput` envelope verbatim. Returns bytes written, or sentinels as above. |

Both host functions follow the same memory-buffer convention (see §6 Marshalling).
The host reads request bytes from `[ptr, ptr+len)`, writes the UTF-8 JSON
response into the caller's output buffer, and returns the number of bytes
written. The snapshot served by `kb_read` is the exact `key_blocks` array the
host bundled into this invocation — there is no cross-call state.

Most modules — including the reference `basic-combat` — read combatant data
straight from `ComputeInput.key_blocks` and do not need host imports at all.
Use `kb_read` / `narrative_query` only when a module needs to look up
*additional* blocks or narrative context beyond what the host pre-selected.

---

## 4. `ComputeInput` envelope structure

The `ComputeInput` envelope is defined in
[`schemas/local-api/compute/compute-input.schema.json`](../../../schemas/local-api/compute/compute-input.schema.json).
It is the **single source of truth** for the input shape — the Rust
`generated::local_api::compute::compute_input::ComputeInput` struct is derived
from it via codegen.

```json
{
  "schema_version": 1,
  "world_ref": {
    "world_id": "w_abc123",
    "branch_id": "root",
    "timeline_head_event_id": "evt_xyz789"
  },
  "key_blocks": [
    { "key_block_id": "kb-def", "block_type": "character", "canonical_name": "Defender",
      "status": "confirmed", "created_at": "...", "body": { "...": "KeyBlock body" } }
  ],
  "narrative_state": {
    "timeline_position": "chapter_3_scene_2",
    "current_chapter": "ch3"
  },
  "invocation": {
    "attacker_id": "kb-atk",
    "defender_id": "kb-def"
  }
}
```

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `schema_version` | integer (`1`) | yes | Envelope version. Must be `1` for V1.x. |
| `world_ref` | object | yes | World and timeline locator: `world_id` (WorldId), `branch_id` (fork branch), `timeline_head_event_id` (current timeline head). |
| `key_blocks` | array of `KeyBlock` | yes | Snapshot of relevant KeyBlocks for this invocation. Each entry is the full wire `KeyBlock` shape from `schemas/domain/key-block.schema.json`, including `body` (which carries `state` for computable blocks — see [entity-scope-model.md](./entity-scope-model.md) §5.5.9). The host selects which blocks to pass based on the module manifest (`required_key_block_types`) and the capability context. |
| `narrative_state` | object | no | Narrative position context — `timeline_position`, `current_chapter`, `current_scene`. Freeform; the shape is module-declared. |
| `invocation` | object | no | Module-defined freeform input parameters. The exact fields are declared in the module's `manifest.json` `schemas.invocation` (V1.62 P1). The host passes them through verbatim. This is the V1 envelope escape hatch for module-specific inputs (e.g., chosen targets, difficulty, dice seed). |

The `key_blocks` array carries the full wire `KeyBlock` type — including the
`state` and `computable` fields added in V1.61. The `state` field is an optional
JSON object nested by `block_type` (compass V1.61 Q5: `state.character.current_hp`).
Only computable KeyBlocks (`computable: true`) participate in WASM compute;
non-computable blocks may still appear in the snapshot for read-only reference.

---

## 5. `ComputeOutput` 4-part envelope

The `ComputeOutput` envelope is defined in
[`schemas/local-api/compute/compute-output.schema.json`](../../../schemas/local-api/compute/compute-output.schema.json).
The module's `compute` export must emit a JSON object with exactly four
top-level keys.

```json
{
  "schema_version": 1,
  "state_delta": [
    { "op": "sub", "path": "character.current_hp",
      "target_key_block_id": "kb-def", "value": 15 }
  ],
  "timeline_events": [
    { "event_id": "...", "event_type": "state_update", "status": "canon", "...": "…" }
  ],
  "new_key_blocks": [],
  "battle_report": { "kind": "combat", "attacker_id": "kb-atk", "defender_id": "kb-def",
    "damage": 15, "defender_hp_before": 100, "defender_hp_after": 85 }
}
```

### 5.1 `state_delta`

Ordered list of `add` / `sub` / `set` operations on nested state paths of
computable KeyBlock bodies (compass V1.61 Q5 dotted-path convention).

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `op` | `"add"` \| `"sub"` \| `"set"` | yes | `add` increments a numeric field; `sub` decrements; `set` replaces any value. |
| `path` | string (dotted) | yes | Dotted state path within the target KeyBlock body (e.g., `character.current_hp`). |
| `target_key_block_id` | string | no | KeyBlock the delta applies to. When omitted, the host applies the delta to the KeyBlock implied by the capability context. |
| `value` | any JSON | no | Value for `set`, or numeric delta for `add`/`sub`. Untyped to allow module-declared state shapes. |

The host applies deltas **in order** to the computable KeyBlock bodies. The
merge resolution semantics (`+/-/set` on nested JSON) are finalized in the
`narrative.compute` capability (P3 T3 of V1.61). The ABI guarantees that the
host applies all deltas atomically: no partial application on error.

### 5.2 `timeline_events`

Array of `TimelineEvent` objects (from `schemas/domain/timeline-event.schema.json`)
to append to the world timeline. Events typically use `event_type: "state_update"`
and `status: "canon"` for compute outcomes. The module should emit a placeholder
`created_at` — the host stamps authoritative timestamps when it persists the
event.

### 5.3 `new_key_blocks`

Array of new `KeyBlock` objects (from `schemas/domain/key-block.schema.json`)
the module creates (e.g., a spawned item, a newly established faction relation).
These are upserted by the host.

### 5.4 `battle_report`

Module-declared freeform report. The `kind` field discriminates the payload
shape (e.g., `"combat"` for casualties, `"economy"` for market prices).
Consumers switch on `kind` to interpret the remaining fields. The schema uses
`additionalProperties: true` — modules may add any fields beyond `kind`.
Per the V1 envelope decision (compass V1.61 Q8), the battle report is kept open
rather than closed to a fixed schema.

### 5.5 Host apply order

The host applies the output in this order:

1. **`state_delta`** — apply `+/-/set` to computable KeyBlock bodies.
2. **`new_key_blocks`** — upsert new KeyBlocks into the World KB.
3. **`timeline_events`** — append events to the timeline.
4. **`battle_report`** — surface to the caller (the `narrative.compute` capability).

This ordering ensures that timeline events can reference freshly upserted
KeyBlocks, and that the battle report reflects the post-delta state.

---

## 6. Marshalling convention

All data exchange between the host and module goes through the module's
**linear memory** as JSON (UTF-8). The ABI uses a pointer+length convention
for input and a pointer+capacity convention for output.

### 6.1 Host → module (input)

The host allocates a buffer by calling the module's `alloc(in_len)`, writes the
`ComputeInput` JSON into `[ptr, ptr+in_len)`, and passes `(ptr, in_len)` to
`compute`.

### 6.2 Module → host (output)

Before calling `compute`, the host allocates a second buffer by calling
`alloc(out_cap)` where `out_cap` is the host's output buffer size (typically
64 KiB — enough for a 4-part combat output). The host passes `(out_ptr, out_cap)`
to `compute`. The module writes the `ComputeOutput` JSON into
`[out_ptr, out_ptr+written)` and returns `written` as `i64`.

### 6.3 Error sentinels

The `compute` export returns `written` as `i64`. Non-negative values indicate
success (bytes written). Negative values are error sentinels:

| Return value | Meaning |
| --- | --- |
| `>= 0` | Success — bytes written to output buffer |
| `-1` | Generic module error |
| `-2` | Output buffer too small (`out_cap` < needed) |

Host functions (`kb_read`, `narrative_query`) use the same convention:

| Return value | Meaning |
| --- | --- |
| `>= 0` | Success — bytes written to output buffer |
| `-1` | Not found / unsupported query |
| `-2` | Output buffer too small |

The host maps `compute` returning `-1` to `ComputeError::ModuleComputeFailed(-1)`
and `-2` to `ComputeError::OutputBufferTooSmall(needed)`. The host maps host
function return values silently — a `-1` from `kb_read` means the block was not
found in the snapshot and the module should handle it gracefully.

### 6.4 Host function memory-buffer ABI

Both host functions follow the same convention so a module never has to guess at
allocation:

```text
nexus::kb_read(id_ptr, id_len, out_ptr, out_cap) -> i64
nexus::narrative_query(q_ptr, q_len, out_ptr, out_cap) -> i64
```

The module owns its linear memory and passes a buffer it allocated for the
result. The host reads request bytes from `[ptr, ptr+len)`, writes the UTF-8
JSON response into `[out_ptr, out_ptr+written)`, and returns `written` as `i64`.
The module must ensure `out_ptr` does not overlap with the request data.

---

## 7. `manifest.json` contract

Every compute module ships a `manifest.json` next to its `.wasm`. The manifest
declares identity, the required input surface, the export names, and optional
sandbox overrides. The exact structure is defined in
[`modules/README.md`](../../../modules/README.md); this section is the normative
reference for the contract fields.

### 7.1 Required fields

| Field | Type | Meaning |
| --- | --- | --- |
| `module_id` | string | Unique module id (must match the directory name). |
| `name` | string | Human-readable name. |
| `version` | string | Module SemVer (independent of the Nexus ABI version). |
| `nexus_abi_version` | integer | Compute envelope ABI version (`1` for V1.x). |
| `required_key_block_types` | array of string | `BlockType` values the module reads (e.g., `["character"]`). The host uses this to select which KeyBlocks to bundle into `ComputeInput.key_blocks`. |
| `compute_export` | string | Name of the WASM export implementing `compute` (§2). |
| `init_export` | string | Name of the WASM export implementing `init` (§2.1). Empty string if none. |

### 7.2 Optional fields

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `description` | string | — | Free-form description. |
| `author` | string | — | Author attribution. |
| `host_functions` | array of string | `[]` | Subset of `["kb_read", "narrative_query"]` the module may call. The host registers only these functions; importing an unlisted function fails instantiation. |
| `battle_report_kind` | string | — | Discriminator the module emits in `battle_report.kind`. |
| `max_fuel` | integer | host default (10M) | Per-invocation fuel override (wasmtime instruction count). |
| `max_memory_mib` | integer | host default (64) | Per-invocation memory-cap override (MiB). |
| `max_wall_time_ms` | integer | host default (30000) | Per-invocation wall-time override (ms). |

### 7.3 `schemas` block (V1.62 P1 — NEW)

V1.62 adds an optional `schemas` block that declares per-module JSON Schema
fragments for host-side validation. The block has four optional sub-objects:

| Sub-object | Validated against | When validated |
| --- | --- | --- |
| `schemas.key_block_attributes` | `HashMap<BlockType, JSON Schema fragment>` | Before invocation: each KeyBlock in `ComputeInput.key_blocks` is validated against `schemas.key_block_attributes[block_type]` if declared. |
| `schemas.key_block_state` | `HashMap<BlockType, JSON Schema fragment>` | Before invocation: each KeyBlock's `body.state` is validated against `schemas.key_block_state[block_type]` if declared. |
| `schemas.invocation` | JSON Schema fragment | Before invocation: `ComputeInput.invocation` is validated against this fragment if declared. |
| `schemas.battle_report` | JSON Schema fragment | After invocation: `ComputeOutput.battle_report` is validated against this fragment if declared. |

Validation failure produces `ComputeError::ManifestValidationFailed { path, detail }`
(see [wasm-host.md](./wasm-host.md) §8).

**Backward compatibility**: omitting the `schemas` block entirely disables
validation — V1.61 modules continue to work unchanged. A module may declare any
subset of the four sub-objects; missing sub-objects skip the corresponding
validation step.

**Fragment policy**: all schema fragments are inline JSON Schema definitions
within the manifest file. `$ref` to external files is not supported in V1.62
(compass V1.62 design item #3 — defer to V2 if needed).

### 7.4 Worked example: basic-combat `schemas` block

```json
{
  "module_id": "basic-combat",
  "name": "Basic Combat",
  "version": "1.0.0",
  "nexus_abi_version": 1,
  "required_key_block_types": ["character"],
  "compute_export": "compute",
  "init_export": "init",
  "host_functions": [],
  "battle_report_kind": "combat",
  "schemas": {
    "key_block_attributes": {
      "character": {
        "type": "object",
        "required": ["max_hp", "base_atk", "base_def"],
        "properties": {
          "max_hp":    { "type": "integer", "minimum": 0 },
          "base_atk":  { "type": "integer", "minimum": 0 },
          "base_def":  { "type": "integer", "minimum": 0 },
          "speed":     { "type": "integer", "minimum": 0 },
          "level":     { "type": "integer", "minimum": 1 }
        }
      }
    },
    "key_block_state": {
      "character": {
        "type": "object",
        "properties": {
          "current_hp":     { "type": "integer", "minimum": 0 },
          "status_effects": { "type": "array", "items": { "type": "string" } },
          "is_alive":       { "type": "boolean" }
        }
      }
    },
    "invocation": {
      "type": "object",
      "properties": {
        "attacker_id": { "type": "string" },
        "defender_id": { "type": "string" }
      }
    },
    "battle_report": {
      "type": "object",
      "required": ["kind", "attacker_id", "defender_id", "damage"],
      "properties": {
        "kind":              { "type": "string", "const": "combat" },
        "attacker_id":       { "type": "string" },
        "defender_id":       { "type": "string" },
        "damage":            { "type": "integer" },
        "defender_hp_before": { "type": "integer" },
        "defender_hp_after":  { "type": "integer" }
      }
    }
  }
}
```

---

## 8. Sandbox model cross-ref

The host enforces three independent sandbox limits on every `compute()` call.
Full details are in [wasm-host.md](./wasm-host.md) §3–§5. Summary:

| Limit | Default | Manifest override | Error variant |
| --- | --- | --- | --- |
| Fuel (instruction count) | 10,000,000 | `max_fuel` | `ComputeError::OutOfFuel` |
| Memory cap | 64 MiB | `max_memory_mib` | `ComputeError::MemoryCapExceeded` |
| Wall-time | 30 seconds | `max_wall_time_ms` | `ComputeError::WallTimeExceeded` |

A module that breaches any limit traps and is reported as a `ComputeError`; it
never crashes the host. The sandbox is per-invocation — each call gets a fresh
instance, so limits always start from a clean slate.

---

## 9. Versioning

### 9.1 `nexus_abi_version`

The ABI version is an integer declared in each module's `manifest.json`. V1.x
uses `nexus_abi_version: 1`. The host rejects modules with an ABI version it
does not recognize. Version increments are **additive** — a new version may
add exports, imports, or envelope fields, but must not remove or change existing
behavior under the same version number.

### 9.2 `schema_version`

The `ComputeInput` and `ComputeOutput` envelopes carry a `schema_version` field
(integer, currently `1`). This is the envelope schema version, distinct from
`nexus_abi_version`. The host uses `schema_version` to detect mismatches between
the module's expected envelope shape and the host's schema.

### 9.3 V2 deferred items

The following items from the V1.61 compass non-goals (§1.2) are deferred to
future major ABI versions (V2.0+). They are **not** supported in the V1 envelope:

| Item | Target |
| --- | --- |
| Multi-module composition / chaining | V2.0+ |
| CDN-based module distribution + Ed25519 signing | V2.0+ |
| Generic Combat Protocol interop certification | V2.0+ |
| Third-party game server integration bridge | V2.0+ |
| GPU compute / SIMD acceleration | V3.0+ |
| Module marketplace / public registry | V3.0+ |
| KB state → human-readable UI editor | V2.0+ |

No new deferred items are added in V1.62 — the V1.62 scope is a course
correction of the V1.61 compute architecture, not an expansion.

---

*Normative Master. V1.62 P2 (2026-06-23). Source material: V1.61 compass grill
decisions Q3/Q6/Q8, `modules/README.md`, `schemas/local-api/compute/`,
`crates/nexus-wasm-host/AGENTS.md`. See `wasm-host.md` for the host runtime side
of this contract.*
