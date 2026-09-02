# nexus-embedding — Embedding Readiness Contract (RN-OGA-3)

Embedding contract crate: the `EmbeddingProvider` trait seam, the embedding
identity tuple, and the fail-closed derived-index protocol. Normative spec:
`.mstar/specs/embedding-readiness.md`.

## Key Rules

- **No OSS embedding execution.** Embeddings are a Nexus Platform-provided
  advanced feature; users never configure embedding APIs. `NoEmbeddings` is
  the only in-repo implementation and always returns
  `EmbeddingError::Unavailable`.
- **Fail-closed protocol.** `verify_index` never returns `Usable` for a stale
  or partial index; lexical fallback is an explicit caller decision. Identity
  mismatch evidence names the changed `IdentityComponent`(s).
- **Zero heavyweight deps.** Runtime dependencies are exactly the
  workspace-pinned `async-trait` + `thiserror`. No `serde` this iteration
  (plain Rust types; additive when a storage/wire consumer lands).
- **No runtime consumer this iteration.** CLI, daemon, and MCA paths must not
  call `embed()`. Index storage lands with the first platform-era consumer
  (DF-78 / Creator Memory).

## Dependencies

- `async-trait` (trait shape, object-safe)
- `thiserror` (`EmbeddingError`)
