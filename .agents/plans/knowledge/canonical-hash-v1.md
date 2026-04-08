# Delta bundle `canonical_hash`

Dev-process knowledge (cross-stack contract). For `docs/` vs `.agents/plans/knowledge/` boundary, see [AGENTS.md](../../../AGENTS.md).

This document is the **cross-stack contract** for how Nexus OSS computes `Bundle.canonical_hash` for idempotency and integrity checks. Private `nexus-platform` must reproduce the same digest for the same logical delta list.

## Preimage (authoritative)

The hash input is **only** the JSON serialization of the bundle’s `**deltas` array**, not the full bundle envelope:

1. Take the `Vec<Delta>` / `Delta[]` in **wire order** (the order stored on the bundle).
2. Serialize with **Serde JSON** to UTF-8 bytes: `serde_json::to_vec(&deltas)` in Rust (`crates/nexus-sync/src/canonical_hash.rs`).
3. Compute **SHA-256** over those bytes.
4. Encode as: `sha256:` + **64 lowercase hex digits** (no `0x` prefix).

Properties:

- Optional fields on `Delta` use Serde’s `skip_serializing_if`; omitted keys **must not** appear in the JSON (they affect the hash).
- Enum values use `snake_case` string tags per generated contracts (`key_block`, `create`, etc.).
- `payload` is serialized as arbitrary JSON (`serde_json::Value`); key order follows Serde’s map serialization (sorted keys for `serde_json::Map`).

For normative wire shapes, see JSON Schema under `schemas/domain/` (e.g. `delta.schema.json`, `bundle.schema.json`).

## Golden vector (frozen)

**Do not change** the fixture without updating platform golden tests and `crates/nexus-sync/src/canonical_hash.rs` (`golden_alignment_vector_matches_documented_digest`).

Single-element array; UTF-8 JSON body (no trailing newline) before hashing:

```json
[{"delta_type":"key_block","operation":"create","target_entity_type":"key_block","payload":{"display_name":"Golden"},"local_timestamp":"2026-04-09T12:00:00Z"}]
```

Expected `canonical_hash`:

```text
sha256:b9c07221605405f763956471055fed2ecdfdce7858f423a371aa387eec8befab
```

## Platform parity checklist

When validating against `nexus-platform`:

- Implement the same preimage (deltas-only, same Serde rules) or document any intentional divergence.
- Run the golden vector above and assert the digest matches.
- Align bundle ingestion idempotency with `idempotency_key` + `canonical_hash` together (see sync contract docs).

Record verification outcomes in the PR or release notes using **neutral placeholders** for internal repo paths (see root `AGENTS.md`).