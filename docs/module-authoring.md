# Module Authoring

A **compute module** is a WebAssembly module that settles deterministic
narrative steps — combat resolution, economy ticks, rule checks — inside a
sandboxed wasmtime host. It is a **stateless pure function**: each invocation
receives a `ComputeInput` envelope and returns a 4-part `ComputeOutput`
envelope; no state carries between calls. It runs **host-local**: the operator
installs it under `~/.nexus42/modules/<id>/`, and a Connect peer can only name
it (module bytes are never peer-supplied). Over the shipped Connect surface
compute is **read-only** — the module never commits state itself (see
[Read-only compute](#read-only-compute)).

The normative ABI contract is
[`.mstar/specs/compute-module-abi.md`](../.mstar/specs/compute-module-abi.md) —
this doc is the authoring reference: the contract at a glance, the
`manifest.json` contract (incl. `wasm_sha256`), the allowlist gate, and the
operator install. The reference implementation to copy is
[`modules/basic-combat/`](../modules/basic-combat/) (`manifest.json`,
`Cargo.toml`, `src/lib.rs`); the module-authoring walkthrough lives in
[`modules/README.md`](../modules/README.md).

## ABI at a glance

Target **`wasm32-unknown-unknown`** (no WASI required). `std` is available on
this target — only I/O, threads, and the wall clock are absent (emit a
placeholder `created_at`; the host stamps authoritative timestamps).

### Exports

| Export | Signature | Required | Purpose |
| --- | --- | --- | --- |
| `memory` | exported linear memory | yes | The host reads input JSON and writes output JSON into this memory. |
| `alloc` | `(len: u32) -> u32` | yes | Allocate `len` bytes in linear memory; return the pointer. |
| `compute` | `(in_ptr: u32, in_len: u32, out_ptr: u32, out_cap: u32) -> i64` | yes | Read `ComputeInput` JSON from `[in_ptr, in_ptr+in_len)`, write `ComputeOutput` JSON to `[out_ptr, out_ptr+written)`, return `written`. Negative returns are error sentinels (below). |
| `init` | `() -> ()` | no | One-shot setup, called once after instantiation when the manifest declares `init_export`. |

The `compute` export **name is configurable** — the manifest's `compute_export`
field declares it; the signatures are fixed. If `init` traps, the invocation
fails immediately (no retry). Full semantics: spec §2–§2.1.

### Host imports (whitelist)

A module may import up to two host functions from the `nexus` namespace. The
host registers **only** the functions the manifest's `host_functions` list
names; importing anything else fails instantiation.

| Import | Signature | Behavior |
| --- | --- | --- |
| `nexus::kb_read` | `(id_ptr: u32, id_len: u32, out_ptr: u32, out_cap: u32) -> i64` | Look up a KnowledgeEntry by ID in the invocation's `key_blocks` snapshot; write its JSON to `out`. Returns bytes written, `-1` if not found, `-2` if `out_cap` too small. |
| `nexus::narrative_query` | `(q_ptr: u32, q_len: u32, out_ptr: u32, out_cap: u32) -> i64` | Return narrative context JSON (V1 passes `narrative_state` through verbatim). Same return convention. |

**Canonical data path:** the host always bundles the relevant KnowledgeEntries
into `ComputeInput.key_blocks` (selected by the manifest's
`required_key_block_types`). Most modules — including `basic-combat` — read
everything they need from that inline snapshot and declare `host_functions:
[]`. Use the host imports only to look up *additional* blocks or context
beyond what the host pre-selected. Spec §3.

### Sandbox

Each `compute()` call runs in a **fresh** wasmtime `Store` + `Instance` —
limits start from a clean slate and nothing leaks across calls (this is why
`alloc` may leak intentionally; see below). Three independent limits; a breach
traps and surfaces as a `ComputeError`, never a host crash. Spec §8.

| Limit | Default | Manifest override |
| --- | --- | --- |
| Fuel (instruction count) | 10,000,000 | `max_fuel` |
| Memory cap | 64 MiB | `max_memory_mib` |
| Wall-time | 30 s | `max_wall_time_ms` |

## Marshalling convention

All exchange goes through the module's linear memory as UTF-8 JSON, with a
pointer+length convention for input and pointer+capacity for output (spec §6):

1. The host calls `alloc(in_len)` to reserve the input buffer and writes the
   `ComputeInput` JSON into `[in_ptr, in_ptr+in_len)`.
2. The host calls `alloc(out_cap)` to reserve the output buffer (`out_cap` is
   typically 64 KiB — enough for a 4-part combat output).
3. The module writes the `ComputeOutput` JSON into
   `[out_ptr, out_ptr+written)` and returns `written`:

| Return | Meaning |
| --- | --- |
| `>= 0` | Success — bytes written to the output buffer |
| `-1` | Generic module error |
| `-2` | Output buffer too small (`out_cap` < needed) |

Host functions use the same convention (`-1` = not found / unsupported query).

### Worked example: `basic-combat` marshalling

[`basic-combat/src/lib.rs`](../modules/basic-combat/src/lib.rs) is the pattern
to reuse:

- **`alloc`** builds a `Vec` and `mem::forget`s it — the leak is intentional
  (the per-invocation instance is discarded right after the call, so there is
  no long-lived leak).
- **`compute`** returns `-2` when the serialized output would exceed
  `out_cap`, copies the output with `ptr::copy_nonoverlapping` (input and
  output buffers are separate allocations), and returns `-1` on any compute
  error (malformed input / missing combatants).
- A `#[global_allocator]` (dlmalloc) is required: `std` provides none on
  `wasm32-unknown-unknown`. The host's memory cap bounds growth.

## Envelopes

### `ComputeInput` (host → module)

Defined in
[`schemas/daemon-api/compute/compute-input.schema.json`](../schemas/daemon-api/compute/compute-input.schema.json)
(spec §4):

| Field | Type | Meaning |
| --- | --- | --- |
| `schema_version` | integer (`1`) | Envelope version. |
| `world_ref` | object | World + timeline locator (`world_id`, `branch_id`, `timeline_head_event_id`). |
| `key_blocks` | array of `KnowledgeEntry` | Snapshot of relevant entries (full wire shape, incl. `body` with `state`). Selected by `required_key_block_types`. |
| `narrative_state` | object | Optional narrative position context; module-declared shape. |
| `invocation` | object | Optional module-defined freeform params (declared in `schemas.invocation`), passed through verbatim. |

### `ComputeOutput` (module → host)

Defined in
[`schemas/daemon-api/compute/compute-output.schema.json`](../schemas/daemon-api/compute/compute-output.schema.json).
Exactly four top-level keys (spec §5):

| Key | Meaning |
| --- | --- |
| `state_delta` | Ordered `add` / `sub` / `set` ops on dotted nested state paths of computable KnowledgeEntry bodies. |
| `timeline_events` | Events to append (`event_type: "state_update"`, `status: "canon"` for compute outcomes; placeholder `created_at`). |
| `new_key_blocks` | New KnowledgeEntries the module creates; the host upserts them. |
| `battle_report` | Module-declared freeform report; `kind` discriminates the payload. |

The host applies the output in this order: `state_delta` → `new_key_blocks` →
`timeline_events` → `battle_report` (events may reference freshly upserted
entries; the report reflects post-delta state). All deltas apply atomically —
no partial application on error.

## `manifest.json`

Every module ships a `manifest.json` **next to its `.wasm`**. Full contract:
spec §7 and [`modules/README.md`](../modules/README.md).

### Required fields

| Field | Type | Meaning |
| --- | --- | --- |
| `module_id` | string | Unique module id. **Must match the directory name** and the install directory (`~/.nexus42/modules/<id>/`). |
| `name` | string | Human-readable name. |
| `version` | string | Module SemVer (independent of the Nexus ABI version). |
| `nexus_abi_version` | integer | Compute envelope ABI version (`1` for V1.x). Unrecognized versions are rejected. |
| `required_key_block_types` | array of string | BlockTypes the module reads (e.g. `["character"]`); the host bundles matching entries into `key_blocks`. |
| `compute_export` | string | Name of the WASM export implementing `compute`. |
| `init_export` | string | Name of the WASM export implementing `init`; empty string if none. |

### Optional fields

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `description` | string | — | Free-form description. |
| `author` | string | — | Author attribution. |
| `host_functions` | array of string | `[]` | Subset of `["kb_read", "narrative_query"]` the module may call. |
| `battle_report_kind` | string | — | Discriminator the module emits in `battle_report.kind`. |
| `max_fuel` | integer | host default (10M) | Per-invocation fuel override. |
| `max_memory_mib` | integer | host default (64) | Per-invocation memory-cap override (MiB). |
| `max_wall_time_ms` | integer | host default (30000) | Per-invocation wall-time override (ms). |
| `wasm_sha256` | string | — | SHA-256 of the compiled `.wasm` bytes this manifest pairs with (see below). |
| `schemas` | object | — | V1.62+: inline JSON-Schema fragments — `key_block_attributes`, `key_block_state`, `invocation`, `battle_report` — validated by the host before/after invocation. Omitting the block disables validation (V1.61 modules keep working unchanged). Spec §7.3. |

### `wasm_sha256` — content-based pairing

`wasm_sha256` is the SHA-256 of the **exact `.wasm` bytes** the manifest ships
with (64 lowercase hex chars). It is **optional but recommended**: when
present, the loader hashes the loaded bytes and rejects a mismatch **before
the pair is compiled or cached** — an old manifest + new `.wasm` always
mismatches, so a mixed pair never enters the module cache and every invocation
fails as a host fault (`internal_error`, "wasm does not match manifest
wasm_sha256").

When the field is **absent** (legacy manifests), the loader falls back to the
stat fence: a size + mtime re-stat around the read that detects a mid-load
swap. The fence **cannot** detect a same-size swap landing outside its
observation windows — another reason to set the hash.

Compute it from the installed artifact:

```bash
shasum -a 256 basic-combat.wasm        # macOS (prints "<hash>  <filename>")
sha256sum basic-combat.wasm            # Linux (prints "<hash>  <filename>")
```

**Embedded modules are auto-paired.** `crates/nexus-wasm-host/build.rs`
compiles the embedded copy and injects `wasm_sha256` computed from the actual
compiled bytes into the staged manifest — the embedded pair is always
content-consistent, and the source `modules/<id>/manifest.json` is left
untouched. (Embedded modules are not reachable over Connect — see
[Operator install](#operator-install).)

## `module_scope` allowlist

Compute is gated by the peer allowlist. A peer may invoke a module only when
its allowlist entry's `module_scope` contains the module id — **missing or
empty `module_scope` denies ALL compute** with `module_not_scoped`, fail-closed,
before any WASM execution:

```json
{
  "peer_ids": [
    {
      "peer_id": "12D3KooW…",
      "world_scope": ["wld_…"],
      "op_scope": ["upsert", "promote", "relate", "check", "assemble", "compute"],
      "module_scope": ["basic-combat"]
    }
  ]
}
```

`module_scope` ids are **host-local module names** (`~/.nexus42/modules/`),
never peer-supplied bytes — the peer names only what the operator installed.
`world_scope` / `op_scope` / `module_scope` are per-peer; denials happen before
any orchestrator call, with zero side effects. The module id for an invocation
resolves from the staged compute session state first, then the entry's
`body.computable.module_id`. Allowlist mechanics and the full scoped-entry
contract: [`nexus-runtime.md`](nexus-runtime.md#allowlist-and-module-scope) and
the [integrator walkthrough](../strategy-samples/README.md).

## Operator install

Modules live under `~/.nexus42/modules/<id>/` (the home the runtime resolved:
`--home` > `NEXUS42_HOME` > `$HOME`):

```
~/.nexus42/modules/<id>/<id>.wasm
~/.nexus42/modules/<id>/manifest.json
```

`<id>` must match both the directory name and the manifest's `module_id`. An
absent or incomplete pair is `module_not_found` at invoke time.

Build and install (mirrors `strategy-samples/README.md` §5; `basic-combat` is
a **standalone crate** — its own `[workspace]` table — so build from inside the
directory):

```bash
cd modules/basic-combat
rustup target add wasm32-unknown-unknown      # once per toolchain
cargo build --release --target wasm32-unknown-unknown

NEXUS_HOME="${NEXUS42_HOME:-$HOME}/.nexus42"
mkdir -p "$NEXUS_HOME/modules/basic-combat"
cp target/wasm32-unknown-unknown/release/basic_combat.wasm "$NEXUS_HOME/modules/basic-combat/basic-combat.wasm"
cp manifest.json "$NEXUS_HOME/modules/basic-combat/"
```

Note the rename: cargo emits the artifact under the crate name
(`basic_combat.wasm`), the store expects the module id (`basic-combat.wasm`).

> **Align `wasm_sha256` (optional but recommended).** The repo-source manifest
> pins the hash of the *embedded* artifact, which differs from any locally
> compiled `.wasm`. A stale `wasm_sha256` rejects every invocation of that
> install. After installing a locally built module, set the field to the hash
> of the installed bytes — or delete it to fall back to the stat fence:

```bash
shasum -a 256 "$NEXUS_HOME/modules/basic-combat/basic-combat.wasm"
# → "<hash>  <filename>" — use only the 64 lowercase hex before the
# two-space filename separator; write it as "wasm_sha256" in the installed
#   manifest.json, or remove the field entirely
```

On the Connect surface the embedded module set is **not** reachable: the
runtime serves only operator-installed modules under `~/.nexus42/modules/`.
The full end-to-end compute walkthrough (stage a session, invoke, receipts):
[`strategy-samples/README.md` §5](../strategy-samples/README.md#5-compute-basic-combat-n-c2-compute-half).

## Read-only compute

Over the shipped Connect surface compute is **read-only by design**:

- `settle: true` is rejected with `settle_not_enabled` — the module never
  commits state itself.
- A `settle: false` response carries no settled `state` map; it surfaces the
  merged computable state view (the confirmed receipt: session state merged
  with the request's `computable` map and the module's `state_delta`).
- **Committing the confirmed result is the caller's job**: persist the receipt
  in your turn ledger and apply world-state changes through the write path
  (`upsert` with world-aware CAS / structured-failure rules — never a forced
  overwrite).

The module's `ComputeOutput` (state deltas, timeline events, new blocks) is
what the host *would* apply in a settling surface; the read-only lane hands the
receipt back to the caller instead. The settle → receipt → narrate discipline
is documented in the
[TRPG turn strategy](../strategy-samples/react-trpg-turn/).

## Next steps

- [Integrator walkthrough](../strategy-samples/README.md) — the E2 loop,
  incl. the compute op.
- [Runtime usage](nexus-runtime.md) — install/run, allowlist + `module_scope`
  setup, home layout.
- [Strategy authoring](strategy-authoring.md) — the strategy side of the loop.
- [Docs index](README.md) — all docs.
- Module guide: [`modules/README.md`](../modules/README.md) — authoring
  walkthrough + embedding procedure.
- Reference implementation: [`modules/basic-combat/`](../modules/basic-combat/).
- ABI spec (normative): [`.mstar/specs/compute-module-abi.md`](../.mstar/specs/compute-module-abi.md).
