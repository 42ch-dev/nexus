# Embedding Readiness Contract (RN-OGA-3)

> **Status:** Normative — V1.181 P0 (readiness-contract form; posture user-locked 2026-09-02 grill-me). Promoted from the iteration-package draft [`../iterations/v1.181/specs/embedding-readiness-contract-draft.md`](../iterations/v1.181/specs/embedding-readiness-contract-draft.md), which remains in the v1.181 package as provenance. Review-chain locks (product-manager → architect, 2026-09-02) are recorded inline as **Lock** notes.
> **Document class:** Master
> **Scope:** the embedding identity tuple, the fail-closed derived-index protocol, the `EmbeddingProvider` trait seam, the `NoEmbeddings` OSS posture, and the platform injection contract. Governs the `crates/nexus-embedding/` contract crate. **No OSS embedding execution.**
> **Coordinates with:** [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md), [world-kb-runtime-architecture.md](world-kb-runtime-architecture.md), [spoke-adapter-architecture.md](spoke-adapter-architecture.md)

## 0. Document Position

This spec is the durable, tracked SSOT for the embedding readiness contract (roadmap row `harness` RN-OGA-3, delivered in readiness-contract form). It records locked contract facts — identity semantics, fail-closed verdicts, trait shape, crate placement — not iteration archaeology. The runtime trigger ("first embedding/vector feature start") moves to the platform era (DF-78) behind this contract; closing RN-OGA-3 does **not** ship DF-78.

## 1. Posture (user-locked, 2026-09-02 grill-me)

Embeddings are a **Nexus Platform-provided advanced feature**. The OSS product ships **no embedding execution**: no provider implementation, no API-key configuration surface, no local model downloads. **Users never research embedding API wiring** — when embeddings arrive, the platform supplies them; the OSS side consumes them through the contract defined here.

**User-visible consequence: none.** No CLI flag, config key, daemon route, or docs that ask an OSS user to pick a provider, paste an API key, or download a model. When embeddings arrive, the platform supplies them; OSS consumes them through this contract.

**Who this is for**

| Audience | This spec's job |
|----------|-----------------|
| OSS lore / memory authors | Never see an embedding config surface. |
| Nexus Platform implementers | Implement `EmbeddingProvider` and inject it; own all config. |
| Future DF-78 / Creator Memory consumers | Depend on the contract; do not invent a second identity or fail-closed rule. |

## 2. Embedding identity

**Lock (architect, v1.181):** the identity tuple is

```text
EmbeddingIdentity = (provider_id, model_id, model_version, dim)
```

- `provider_id` — stable identifier of the serving provider (platform-side namespace).
- `model_id` — the provider's model identifier.
- `model_version` — provider-owned opaque version token. It **MUST change whenever the served weights change** — fine-tune, quantization, truncation, or any other alteration of vector semantics. This subsumes research `research-opengameagent.md` §7 P0-2's `provider/model/weights/dim` tuple: weights identity is folded into `model_version` rather than carried as a separate component; the fail-closed semantics the research requires are unchanged.
- `dim` — declared vector dimension.

Semantics (normative):

- Every derived vector index records the identity it was built with (the **populate stamp**).
- Identity comparison is structural equality over all four components; **any** component difference invalidates the index.
- On mismatch, the verdict evidence names the changed component(s) — `IdentityComponent::{ProviderId, ModelId, ModelVersion, Dim}` — so operators see *why* a rebuild is required.
- **Serialization:** identity and protocol types are plain Rust types with **no `serde` derives this iteration** (Lock — architect, v1.181): no storage or wire consumer exists yet; adding serde later is additive and breaks nothing. Index persistence schema lands with the first platform-era consumer.

Rationale (research §7 P0-2): mixing vectors from different models/versions silently corrupts similarity semantics; the only safe behavior is explicit rebuild.

## 3. Derived-index protocol (fail-closed)

A derived vector index is **rebuildable data**; the authoritative store is the World KB (lore entries) / memory fragments. Protocol:

1. **Populate**: the platform-era consumer embeds entries and writes index records, each stamped with `EmbeddingIdentity`; completing a populate pass writes the index's **populate stamp** (identity + scope-completeness marker).
2. **Verify** (before any vector read): `verify_index(expected, index) -> IndexVerdict`

   ```text
   IndexVerdict = Usable | RebuildRequired(RebuildReason) | Unavailable
   RebuildReason = IdentityMismatch(IdentityMismatch) | IncompleteScope
   ```

   - `Usable` — the populate stamp's identity equals the expected identity and the scope is complete.
   - `RebuildRequired(IdentityMismatch(..))` — any identity component differs; evidence names the changed component(s). **Fail-closed: no vector read is served from a stale index.**
   - `RebuildRequired(IncompleteScope)` — the populate stamp matches but the scope is incomplete (populate interrupted or partial).
   - `Unavailable` — no provider, or **no populate stamp at all** (index never built).
   - An absent expected identity (`None` — no provider in the composition root, e.g. `NoEmbeddings`) yields `Unavailable` regardless of the index state: an unverified store is never served.

3. **Rebuild**: explicit, consumer-driven re-embed after `RebuildRequired`. Never automatic on the read path.
4. **Lexical fallback is explicit**: when the verdict is not `Usable`, the caller decides keyword-only operation and must be able to *state* it (trace/inspector note). The protocol never silently degrades.

**Empty-index lock (architect, v1.181 — resolves the draft §3 open lock):** an index whose populate stamp matches the expected identity and whose scope is complete is `Usable` **even with zero vector records** — a freshly scoped world legitimately has no entries, and forcing `RebuildRequired` on empty is a liveness trap (rebuild produces the same empty index). Bare absence of records **without** a populate stamp is `Unavailable` ("no index at all"), not `RebuildRequired`. Fail-closed is preserved: the populate stamp is the verification anchor, so an unverified store is never served.

## 4. `EmbeddingProvider` trait (seam)

**Lock (architect, v1.181) — trait shape:**

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn identity(&self) -> &EmbeddingIdentity;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError>;
}
```

- **Async, object-safe.** The platform implementation is network-backed; a synchronous trait would be a stopgap the platform era would have to replace. `async_trait` is the workspace-pinned, repo-standard mechanism (used by `nexus-spoke-adapter`, `nexus-knowledge`, …), keeps the trait object-safe for `Arc<dyn EmbeddingProvider>` sharing, and adds no new external dependency. `Send + Sync` supertraits are required: the provider is shared across async daemon tasks.
- **OSS ships `NoEmbeddings`** — the only implementation in this repo — returning `EmbeddingError::Unavailable`. It documents the OSS posture and lets consumers be written against the trait before the platform lands.
- **Platform injection point (no OSS config surface):** the platform constructs its provider and injects it as `Arc<dyn EmbeddingProvider>` at the **daemon composition root**, passing it into the moment-context-assembly / future memory consumers. Provider construction, credentials, endpoints, and model selection are owned **entirely platform-side**. The OSS config surface gains **nothing**: no config keys, no environment variables, no CLI flags, no docs referencing providers, API keys, provider URLs, or model downloads. When no platform injection exists, the constructed value is `NoEmbeddings`.
- `dim` is part of identity; providers must not return vectors whose length ≠ declared `dim` (the verifier treats this as corruption → `RebuildRequired`).

## 5. Contract crate placement

**Lock (architect, v1.181):** new workspace crate **`crates/nexus-embedding/`** — a neutral, dependency-light contract crate confirmed over relocation into an existing seam.

Rationale (dep-graph facts, v1.181): the two planned consumers are lore activation (`nexus-moment-context-assembly`, DF-78 era) and Creator Memory (`nexus-creator-memory`, which already reserves `memory_item.embedding_ref`). `nexus-creator-memory` does **not** depend on `nexus-knowledge`; hosting the contract in `nexus-knowledge` would pull that crate's weight (`tokio`, `uuid`, `spoke-schemas`, `async-trait`) into Creator Memory's graph for a pure-types contract. A dedicated zero-heavy-dep crate lets both consumers depend on the contract without cross-domain coupling.

Dependency posture (normative):

- Allowed dependencies: workspace-pinned **`async-trait`** (trait shape, §4) and **`thiserror`** (`EmbeddingError`). Both are already workspace-pinned — no new external dependency.
- **No `serde`** this iteration (§2); plain Rust types only.
- The crate compiles and tests with **zero I/O**; no daemon route, no DB migration, no runtime caller this iteration. Index *storage* lands with the platform-era consumer; this crate owns the contract types + protocol logic.

## 6. Consumers (planned, not wired in v1.181)

| Role | Authoritative store | Derived index | Status this iteration |
|------|--------------------|---------------|------------------------|
| Lore activation (DF-78, platform era) | World KB entries | World-scoped lore vector index | **Consumer (planned)** — contract only; no MCA / activation wiring |
| Creator Memory | `memory_fragments` (`embedding_ref` seam reserved in `memory_item.rs`) | memory vector index | **Future consumer** — contract reusable; seam stays reserved, not wired |
| OSS `NoEmbeddings` | — | — | **Seam occupant, not a consumer** — only in-repo impl; returns `Unavailable`; documents the OSS posture |

No additional consumer is in scope. CLI, daemon, and MCA paths must not call `embed()` this iteration.

## 7. Non-goals

- No wire DTOs, no DB migrations, no daemon routes (storage schema lands with the first real consumer).
- No provider implementation beyond `NoEmbeddings` in this repo.
- No user-facing embedding config (API keys, provider URLs, model downloads, model-id pickers).
- No DF-78 vector lore activation and no Creator Memory embedding wiring (platform / later era).
- No similarity/search algorithm lock-in (platform-era consumer choice); the contract governs identity + lifecycle, not ANN strategy.
- No OSS-side injection/config ownership — those stay platform-side.
- No serde/serialization schema this iteration — additive when a storage consumer exists.

## 8. Roadmap close (product)

When this contract ships, [`harness` RN-OGA-3](../projects/harness/roadmap.md) is closeable: Done-definition (identity invalidates index; mismatch fail-closed; lexical fallback explicit) is satisfied in contract form. [`harness` DF-78](../projects/harness/roadmap.md) stays open; annotate its gate to "RN-OGA-3 contract locked v1.181". Runtime trigger moves with DF-78 to the platform era.
