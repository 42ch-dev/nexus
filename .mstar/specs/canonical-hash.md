# Delta bundle `canonical_hash` (OSS companion)

**Status**: Normative (OSS implementation notes)  
**Document class**: Companion  
**Platform authority**: nexus-platform `v1-spec/adr/adr-006-bundle-canonical-hash.md`

Implementation notes for this repository. Normative definition: nexus-platform `v1-spec/adr/adr-006-bundle-canonical-hash.md`.

## Bundle content digest

The digest covers **only** the JSON serialization of the bundle’s **`deltas` array** as UTF-8 bytes — **not** the full envelope (no `bundle_id`, `idempotency_key`, `base_versions`, etc.).

1. Take `Vec<Delta>` / `Delta[]` in **wire order** (same order as on the bundle).
2. Serialize with **Serde JSON**: `serde_json::to_vec(&deltas)` — reference: `crates/nexus-cloud-sync/src/canonical_hash.rs`.
3. **SHA-256** over those bytes.
4. Encode: `sha256:` + **64 lowercase hex digits** (no `0x` prefix).

**Serialization:** optional fields omitted per Serde `skip_serializing_if`; enums as `snake_case` wire strings; `payload` maps with sorted keys. `local_timestamp` serializes as `DateTime<Utc>` (RFC 3339 with `+00:00` suffix) — a typify-codegen change that updated the frozen digest below. Other stacks must match Rust bytes (golden vector below).

Wire shapes: `schemas/platform/sync/` (`delta.schema.json`, `bundle.schema.json`).

## Two concepts (do not conflate)

| Concept | Meaning | Typical location |
| -------- | -------- | ---------------- |
| **Bundle content digest** | Hash of **only** `deltas[]` | Bundle `canonical_hash`, platform `SyncCommand.canonical_hash` |
| **Graph provenance tag** | Neo4j placeholder `sha256:<bundleId>:<entityId>` | Neo4j node property `canonical_hash` |

Context Assembly / graph reads default to the **graph tag** unless stated otherwise.

## Golden vector (frozen)

Do not change without updating platform golden tests and the OSS implementation crate
(`crates/nexus-cloud-sync/src/canonical_hash.rs`,
test `golden_alignment_vector_matches_documented_digest`). The vector below is the
same frozen cross-stack fixture the Rust test uses (no `target_entity_type` — the
optional field is omitted in serialization).

```json
[{"delta_type":"key_block","operation":"create","payload":{"display_name":"Golden"},"local_timestamp":"2026-04-09T12:00:00Z"}]
```

Expected digest:

```text
sha256:23f370c5ec4797f194b9fbbdba556c4de1d7c18c60b14a64898b1919486969fe
```

## References

- `crates/nexus-cloud-sync/src/canonical_hash.rs`
- [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md) §3.7 — `nexus-sync` → `nexus-cloud-sync`
- `schemas/platform/sync/bundle.schema.json`
- nexus-platform `v1-spec/shared/schema/bundle-envelope-schema-v1.md`, `v1-spec/cli-sync/sync-contract-v1.md`, `v1-spec/consistency/consistency-rules-v1.md`
