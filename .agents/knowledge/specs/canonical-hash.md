# Delta bundle `canonical_hash` (OSS companion)

Dev-process spec for this repository. For layout, see [specs/README.md](README.md) and [knowledge/AGENTS.md](../AGENTS.md).

## Normative SSOT (v1-spec)

**Bundle content digest** (信封 / `SyncCommand` / API 请求体上的 `canonical_hash` 幂等摘要) 的 **规范性定义** 以私有规格树 **v1-spec** 中的 **ADR-006: DeltaBundle `canonical_hash` preimage、跨栈一致性与平台映射** 为准（路径：`adr/adr-006-bundle-canonical-hash.md`，通过项目 `AGENTS.md` 所述 `specs_root` / `local-paths.json` 读取）。

本文档 **不** 与 ADR-006 并列作为真源：若叙述不一致，**以 ADR-006 更新为准**；此处提供 **nexus 开源仓** 内的实现指针、与 ADR §3.3 **一致** 的黄金向量副本，以及跨仓协作检查项。

---

## Bundle content digest（与 ADR-006 §3 对齐）

This section mirrors **ADR-006 §3** for developers working **only** in the OSS repo.

The digest covers **only** the JSON serialization of the bundle’s **`deltas` array** as UTF-8 bytes — **not** the full envelope (no `bundle_id`, `idempotency_key`, `base_versions`, etc.).

1. Take `Vec<Delta>` / `Delta[]` in **wire order** (same order as on the bundle).
2. Serialize with **Serde JSON**: `serde_json::to_vec(&deltas)` — reference implementation: `crates/nexus-sync/src/canonical_hash.rs`.
3. **SHA-256** over those bytes.
4. Encode: `sha256:` + **64 lowercase hex digits** (no `0x` prefix).

### Serialization semantics (ADR-006 §3.2)

- **Optional fields**: Serde `skip_serializing_if` (or equivalent) — omitted keys **must not** appear in JSON; presence affects the digest.
- **Enums**: `snake_case` wire strings per contracts (`key_block`, `create`, …).
- **`payload`**: arbitrary JSON; `serde_json::Map` uses **sorted keys**. **TypeScript and other stacks must reproduce the same byte sequence as Rust**; do **not** assume unchecked `JSON.stringify` matches Serde without golden-vector proof.

For wire shapes, see `schemas/domain/` (`delta.schema.json`, `bundle.schema.json`).

---

## Two different “canonical_hash” concepts (ADR-006 §D2)

| Concept | Meaning | Typical location |
| -------- | -------- | ---------------- |
| **Bundle content digest** | Hash of **only** `deltas[]` per §3 above | Bundle `canonical_hash`, platform Postgres `SyncCommand.canonical_hash` |
| **Graph entity provenance tag** | Neo4j Phase B **placeholder** `sha256:<bundleId>:<entityId>` — **not** content-addressed | Neo4j node property `canonical_hash` |

They **must not** be conflated. Context Assembly / graph reads default to the **graph tag** unless stated otherwise. **Neo4j-only** operational detail lives in the **nexus-platform** repo under its own `plans/knowledge/canonical-hash.md` (ADR-006 §4.4).

---

## Golden vector (frozen, ADR-006 §3.3)

**Do not change** without coordinated updates to ADR-006 §3.3, platform CI golden tests, and `crates/nexus-sync/src/canonical_hash.rs` (`golden_alignment_vector_matches_documented_digest`).

Single-element array; UTF-8 JSON **no trailing newline**:

```json
[{"delta_type":"key_block","operation":"create","target_entity_type":"key_block","payload":{"display_name":"Golden"},"local_timestamp":"2026-04-09T12:00:00Z"}]
```

Expected **Bundle content digest**:

```text
sha256:b9c07221605405f763956471055fed2ecdfdce7858f423a371aa387eec8befab
```

---

## Platform parity checklist

When validating against **nexus-platform** (private):

- [ ] Implement the same preimage (deltas-only, byte-identical serialization to Rust).
- [ ] Assert the golden vector above in CI (ADR-006 §3.3 **验证义务**).
- [ ] Align idempotency with **`idempotency_key` + Bundle content digest** (see ADR-006 §4.2 and v1-spec `consistency-rules-v1.md` / `sync-contract-v1.md`).
- [ ] HTTP / OpenAPI: optional **`canonicalHash`** (camelCase) must match `/^sha256:[a-f0-9]{64}$/` when present (ADR-006 §4.3).

Record outcomes in PRs using **neutral placeholders** for internal paths (root `AGENTS.md`).

---

## References (this repo)

- `crates/nexus-sync/src/canonical_hash.rs` — reference implementation.
- `schemas/domain/bundle.schema.json` — `canonical_hash` field shape; preimage scope per ADR-006.

**External (v1-spec, via local-paths):** ADR-006, `schema/bundle-envelope-schema-v1.md`, `cli-sync/sync-contract-v1.md`, `consistency/consistency-rules-v1.md`.
