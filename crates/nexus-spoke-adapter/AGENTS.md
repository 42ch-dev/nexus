# nexus-spoke-adapter

The **only** crate boundary that crosses between nexus domain concerns and SPOKE standard objects.

## Purpose

Since V1.145 P1b this crate is the **capability aggregation** layer (spec §7.4 / §8). It owns four surfaces that together form the single spoke boundary:

1. **Typed accessors** over the `extensions.nexus` namespace on a spoke `KnowledgeEntry` (7 fields: `world_id` / `character_id` / `actor_world_binding_id` — exactly one narrative owner key, v1.184 — plus `created_from_command_id`, `source_work_id`, `source_chapter`, `source_provenance_kind`). The retired World-only visibility bool `creator_only` is **not** a typed key and **not** an unknown passthrough: it is refused on raw input with the stable `legacy_creator_only_unsupported` reason (`false` included) and removed by every typed write. See `src/extensions.rs`.
2. **Lifecycle delegation (Surface A)** — delegates standard lifecycle invariants to `spoke-operations` (validate/apply promote, status transitions, assemble packet, extension merge, revision assert). Where `spoke-operations` exports a function, this crate re-exports or wraps it; it never reimplements a lifecycle invariant. See `src/ops.rs`.
3. **Conversion seam** — the sole `KnowledgeEntryRecord` ↔ spoke `KnowledgeEntry` conversion (`knowledge_record_to_spoke` / `spoke_to_knowledge_record` + `KnowledgeEntryRecordSpokeExt`; renamed with the v1.184 owner-aware aggregate), owned here since V1.145 P1a (moved out of `nexus-knowledge` per the orphan rule). Since v1.191 P1 this seam also maps the native holder governance (`KnowledgeEntryRecord.holder_entry_id` / `.disclosure`) exactly to the spoke `KnowledgeEntry.owner` / `.disclosure` core fields. See `src/conversion/`.
4. **Production adapter home** — `NexusAdapter` (V1.146 rename) + 6 spoke port impls in `src/adapter/` (V1.145 P1b rehome from `nexus-local-db`). Consumes `nexus-local-db` storage primitives and implements spoke's port traits (natively `async fn` since spoke-operations 0.9.1) over async `SQLite` I/O.

Since V1.141 the crate also flat-re-exports spoke 0.4.0's adapter **port traits + `orchestrate_*` entrypoints + operand wire types** (Surface B, spec §7.3) so consumers implement spoke's ports and call spoke's orchestrators through this single import boundary — pure pass-through, no nexus logic.

## Layering (post-V1.145 P1b)

| Crate | Role | Dep edge |
|-------|------|----------|
| `nexus-local-db` | **Pure storage** — DB CRUD primitives only; no spoke-adapter dep | ← `nexus-spoke-adapter` depends on it |
| `nexus-spoke-adapter` | **Capability aggregation** — adapter + 6 ports + Surface A/B | → depends on `nexus-local-db`, `nexus-knowledge`, `spoke-schemas`, `spoke-operations` |

## Authority

- Normative spec: [`specs/spoke-adapter-architecture.md`](../../.mstar/specs/spoke-adapter-architecture.md) (tracked). §7.2 is the authoritative public API surface; §7.3 is the Surface B (ports + orchestrators) surface; §2 is the `extensions.nexus` contract.
- Upstream types: `spoke-schemas` + `spoke-operations` (crates.io, lockstep exact pin on the **pinned upstream lockstep release** — the version SSOT is the workspace manifest, mirrored by the two root npm pins and `tooling/check-wire-drift.sh::SPOKE_PIN`).
- **Shipped (v1.191 P1):** the lockstep SPOKE release plus complete Actor-holder governance (native `holder_entry_id`/`disclosure`, Creator management vs ActorView admission, `creator_only` cutover) and the production extraction wrapper. Durable contract: [`.mstar/specs/holder-governance.md`](../../.mstar/specs/holder-governance.md). `spoke-connect` stays behind `connect-host` / `connect-client`.

## Key rules

- **No lifecycle reimplementation (Q13).** Where `spoke-operations` exports a function, this adapter re-exports or wraps it. Do NOT reimplement any lifecycle invariant here. A wrapper that renames (`apply_promote` → `apply_promote_acceptance`) is fine; a wrapper that re-checks the promote gate is not. The lifecycle-delegation surface (Surface A) is pass-through over `spoke-operations`; the production adapter (surface 4) maps spoke ↔ storage, it does not re-derive spoke invariants.
- **Call-boundary invariant (HARD, spec §7).** Every public function accepts/returns spoke standard types only (`KnowledgeEntry`, `Finding`, `PromoteRequest`, `AssemblePacket`, `ExtensionMap`, `SpokeResult`). There are no nexus wrapper types in this crate — the adapter IS the boundary.
- **Round-trip preservation (spec §2.2).** Unknown namespaces and unknown keys inside `extensions.nexus` are preserved verbatim. Empty `extensions.nexus` is valid and not dropped. The typed accessors touch only the 7 known keys under the `"nexus"` namespace; the retired `creator_only` key is classified as known only so it can never ride out as product-local extras.
- **`extensions` newtype key.** `KnowledgeEntry.extensions` is keyed by the typify-generated `KnowledgeEntryExtensionsKey` newtype (regex-validated `^[a-z][a-z0-9_-]*$`), not plain `String`. It does not implement `Borrow<str>`, so namespace lookups must construct the key via `KnowledgeEntryExtensionsKey::try_from("nexus")`.

## Holder governance (v1.191 P1 — shipped)

This crate is the single place where native holder governance crosses the SPOKE wire:

- **Exact field mapping.** `KnowledgeEntryRecord.holder_entry_id` → spoke `KnowledgeEntry.owner`, and `.disclosure` → `.disclosure`. No governance value rides in `extensions.nexus`, and the wire `owner` is never a narrative-container id (durable §1, D2).
- **Scoped ports (HARD).** `NexusAdapter::new(pool, KnowledgeReadScope)` requires a validated request-bound scope for every KE load/mutate/query path; `NexusAdapter::new_host(pool)` is tools/metadata only and fails closed on every KE entry point. Relationship/finding/compute expansion filters or rejects hidden operands **before** emitting an id or payload, and a missing scope never widens selection. See `.mstar/specs/holder-governance.md` §4.
- **Retired legacy key.** Raw input that still carries `extensions.nexus.creator_only` is refused with the stable `legacy_creator_only_unsupported` reason (`false` included) — never folded into governance, never carried as an unknown extra.
- **Production extraction wrapper.** `src/extraction.rs` owns `ResolvedExtractionPort` + the native callback and is the only caller of `spoke_operations::adapter::orchestrate_extract`; `ExtractionPort` stays outside `BaselinePorts`/`FullPorts`. The adapter imports neither `nexus-core` nor `nexus-orchestration`. See durable §8.
- **Connect manifest.** `src/manifest.rs` declares `ke-ownership` only on the composition that enforces every served family, and never declares `ke-extraction` (remote extract is not served). See durable §9.

## Dependencies

- `spoke-schemas`, `spoke-operations` (workspace, lockstep exact pin on the pinned upstream release)
- `serde`, `serde_json`
- `nexus-knowledge` (domain types + conversion seam, V1.145 P1a)
- `nexus-local-db` (storage primitives — `SqliteKbStore`, `open_pool`, `run_migrations`, CAS helpers; V1.145 P1b adapter rehome)
- `sqlx`, `tokio` (SQLite I/O; adapter port impls are natively `async fn` — no sync bridge)

Dev-deps mirror the runtime deps so tests can compare wrapper output against the underlying spoke function directly; `tempfile` for the adapter's `#[cfg(test)]` SQLite fixtures.

## V1.146 P5 sweep notes

- The adapter now hosts 13 modules in `src/adapter/` (activation, computable_port, computable_port_stub, finding_port, fork_port, host_manifest_port, knowledge_entry_port, mca_read, mind_state, narrative_read, relation_port, rule_query_port, scope_query_port) plus the free-function conversion seam in `src/conversion/`. See `.mstar/specs/spoke-adapter-architecture.md` §7.4 for the production-vs-stub matrix.
- `activation` is the **default-on lore activation engine** (V1.149 / DF-74) — pure match + Relation hop expand; supersedes the V1.146 flag-gated spike. MCA calls the engine; CLI loads hop edges; no matching/hop code in `spoke-operations`.
- `build_assemble_packet` exposes the spec §7.2 signature. Spoke's real API takes a `BuildAssemblePacketInput` struct with `&[KnowledgeEntryForAssemble]` and a packet-level `extensions` slot. The wrapper honors §7.2 and wraps internally. See `src/ops.rs` doc comment.
