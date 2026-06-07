# Delta bundle `canonical_hash` (OSS companion)

**Status**: Normative (OSS implementation notes)  
**Document class**: Companion  
**Platform authority**: nexus-platform `v1-spec/adr/adr-006-bundle-canonical-hash.md`

Implementation notes for this repository. Normative definition: nexus-platform `v1-spec/adr/adr-006-bundle-canonical-hash.md`.

## Bundle content digest

The digest covers **only** the JSON serialization of the bundle’s **`deltas` array** as UTF-8 bytes — **not** the full envelope (no `bundle_id`, `idempotency_key`, `base_versions`, etc.).

1. Take `Vec<Delta>` / `Delta[]` in **wire order** (same order as on the bundle).
2. Serialize with **Serde JSON**: `serde_json::to_vec(&deltas)` — reference: `crates/nexus-cloud-sync/src/canonical_hash.rs` (workspace crate may still be named `nexus-sync` until V1.21 rename).
3. **SHA-256** over those bytes.
4. Encode: `sha256:` + **64 lowercase hex digits** (no `0x` prefix).

**Serialization:** optional fields omitted per Serde `skip_serializing_if`; enums as `snake_case` wire strings; `payload` maps with sorted keys. Other stacks must match Rust bytes (golden vector below).

Wire shapes: `schemas/domain/` (`delta.schema.json`, `bundle.schema.json`).

## Two concepts (do not conflate)

| Concept | Meaning | Typical location |
| -------- | -------- | ---------------- |
| **Bundle content digest** | Hash of **only** `deltas[]` | Bundle `canonical_hash`, platform `SyncCommand.canonical_hash` |
| **Graph provenance tag** | Neo4j placeholder `sha256:<bundleId>:<entityId>` | Neo4j node property `canonical_hash` |

Context Assembly / graph reads default to the **graph tag** unless stated otherwise.

## Golden vector (frozen)

Do not change without updating platform golden tests and the OSS implementation crate (`crates/nexus-cloud-sync/src/canonical_hash.rs`; today often `crates/nexus-sync/src/canonical_hash.rs`) (`golden_alignment_vector_matches_documented_digest`).

```json
[{"delta_type":"key_block","operation":"create","target_entity_type":"key_block","payload":{"display_name":"Golden"},"local_timestamp":"2026-04-09T12:00:00Z"}]
```

Expected digest:

```text
sha256:b9c07221605405f763956471055fed2ecdfdce7858f423a371aa387eec8befab
```

## References

- `crates/nexus-cloud-sync/src/canonical_hash.rs` (target name; today: `crates/nexus-sync/src/canonical_hash.rs`)
- [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md) §3.7 — `nexus-sync` → `nexus-cloud-sync`
- `schemas/domain/bundle.schema.json`
- nexus-platform `v1-spec/shared/schema/bundle-envelope-schema-v1.md`, `v1-spec/cli-sync/sync-contract-v1.md`, `v1-spec/consistency/consistency-rules-v1.md`
