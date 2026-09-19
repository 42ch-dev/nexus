# nexus-core

Owned transport-neutral World KB service for graph/patch/candidates/changes.

- Public API: `CoreService`, `Principal`, `CoreOpenOptions`, `CoreAccess`, `CoreError`.
- No daemon, Axum, napi, orchestration, or SQL pool in the public surface.
- Depends on `nexus-spoke-adapter` with `default-features = false` (no WASM compute).

## Actor holder governance (v1.191 P1 — shipped)

Core is the admission authority for holder-scoped knowledge; the durable
contract is [holder-governance.md](../../.mstar/specs/holder-governance.md) §§3–5:

- **Admission, not client input.** `actor_knowledge` builds a private-field
  non-`Serialize` `AdmittedKnowledgeContext` only after the `Principal`, stored
  owner and `AdmittedActorContext` checks, with one server-chosen policy:
  `CreatorManagement` (owned containers, including known private rows, for review)
  or `ActorView` (exact admitted holder plus authorized containers — every Connect
  path uses this one and never inherits management review).
  `ActorKnowledgeViewService::actor_view_scope` resolves the stored
  ownership/container rows plus the `nexus-local-db` holder registry into the
  lower-layer `KnowledgeReadScope`.
- **Selection before observation.** `view` / `list` / detail / search apply the
  container + disclosure predicates before the keyset cursor, `LIMIT`, count,
  ranking and snippet truncation, so a hidden row is indistinguishable from an
  absent one.
- **Authoring under CAS.** `world_kb` resolves the closed `audience` from the
  admitted identity and writes it with the content under `expected_revision`;
  ordinary content / promote / compute writes preserve the stored governance, and
  WorldSheet linking plus linked-entry privacy edits reject `invalid_world_sheet`
  in both directions instead of silently unlinking.
- **Session invalidation.** `actor_fence` adds a typed World knowledge lease in the
  existing module (World ids then Character ids, lexical order, nonblocking busy
  refusal); `actor_sessions::ActorSessionKey` carries the read policy and the
  World/Character `knowledge_revision` pair, so a stored revision mismatch retires
  the stale session through the existing tombstone machinery — the lifecycle epoch
  is not overloaded.
- **Pack identity.** `world_pack` owns holder-map adoption and quarantine: a foreign
  holder id is never auto-claimed by equality, unmapped/unknown governance stays
  quarantined, and `--review-import` is an owner-only bounded read arm.
