# nexus-spoke-adapter

The **only** crate boundary that crosses between nexus domain concerns and SPOKE standard objects.

## Purpose

Two responsibilities, nothing more:

1. **Typed accessors** over the `extensions.nexus` namespace on a spoke `KnowledgeEntry` (5 fields: `world_id`, `created_from_command_id`, `source_work_id`, `source_chapter`, `source_provenance_kind`). See `src/extensions.rs`.
2. **Thin delegation** of standard lifecycle invariants to `spoke-operations` (validate/apply promote, status transitions, assemble packet, extension merge, revision assert). See `src/ops.rs`.

Since V1.141 the crate also flat-re-exports spoke 0.4.0's adapter **port traits + `orchestrate_*` entrypoints + operand wire types** (Surface B, spec §7.3) so consumers implement spoke's ports and call spoke's orchestrators through this single import boundary — pure pass-through, no nexus logic.

## Authority

- Normative spec: [`specs/spoke-adapter-architecture.md`](../../.mstar/specs/spoke-adapter-architecture.md) (tracked). §7.2 is the authoritative public API surface; §7.3 is the Surface B (ports + orchestrators) surface; §2 is the `extensions.nexus` contract.
- Upstream types: `spoke-schemas` + `spoke-operations` (crates.io, lockstep exact pin `=0.5.0`).

## Key rules

- **Thin facade (Q13).** Where `spoke-operations` exports a function, this adapter re-exports or thin-wraps it. Do NOT reimplement any lifecycle invariant here. A wrapper that renames (`apply_promote` → `apply_promote_acceptance`) is fine; a wrapper that re-checks the promote gate is not.
- **Call-boundary invariant (HARD, spec §7).** Every public function accepts/returns spoke standard types only (`KnowledgeEntry`, `Finding`, `PromoteRequest`, `AssemblePacket`, `ExtensionMap`, `SpokeResult`). There are no nexus wrapper types in this crate — the adapter IS the boundary.
- **Round-trip preservation (spec §2.2).** Unknown namespaces and unknown keys inside `extensions.nexus` are preserved verbatim. Empty `extensions.nexus` is valid and not dropped. The typed accessors touch only the 5 known keys under the `"nexus"` namespace.
- **`extensions` newtype key.** `KnowledgeEntry.extensions` is keyed by the typify-generated `KnowledgeEntryExtensionsKey` newtype (regex-validated `^[a-z][a-z0-9_-]*$`), not plain `String`. It does not implement `Borrow<str>`, so namespace lookups must construct the key via `KnowledgeEntryExtensionsKey::try_from("nexus")`.

## Dependencies

- `spoke-schemas`, `spoke-operations` (workspace, `=0.5.0`)
- `serde`, `serde_json`

Dev-deps mirror the runtime deps so tests can compare wrapper output against the underlying spoke function directly.

## Known concerns (open at V1.139 P0)

- `build_assemble_packet` exposes the spec §7.2 signature `(packet_id, &[KnowledgeEntry], max_entries)`. Spoke's real API takes a `BuildAssemblePacketInput` struct with `&[KnowledgeEntryForAssemble]` and a packet-level `extensions` slot. The wrapper honors §7.2 and wraps internally (`extensions: None`). If a future caller needs packet-level extensions, amend §7.2 rather than growing this wrapper. See `src/ops.rs` doc comment.
