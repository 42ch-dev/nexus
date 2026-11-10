# Reference Store Layout (Retired)

## Scope

Former User-scoped reference registry in SQLite and its `body.md` tree on disk — retired V1.231

## Current state

| Attribute | Value |
| --- | --- |
| **Status** | **Retired (V1.231)** — no live surface. Kept only as the retirement pointer. |
| **Document class** | Master (retired) |
| **Scope** | The former reference registry (`reference_sources`) and its `references/units/<id>/body.md` tree |
| **Retired by** | [ADR K1 §3.2](../../../.agents/adr/proposed/architecture/2026-11-10-retire-user-scope-and-references.md); [local-db-schema.md](local-db-schema.md) §4.1.1 |

### 1. What was retired

The two-layer model is gone in full: the `reference_sources` registry table in workspace `state.db`, the canonical `~/.nexus42/creators/<creator_id>/references/units/<reference_source_id>/body.md` bodies, the `nexus42 creator reference register|list|show` CLI group, `CoreService::{register,list,get}_reference`, the `GET /v1/daemon/references[/{id}]` routes, and the `nexus-home-layout` reference-only path helpers.

Data handling followed ADR K1 §3.2: rows and bodies were inventoried and archived outside the repo, archived unit directories were deleted, then `reference_sources` was dropped by migration `20261111000001_retire_user_and_reference_stores.sql`. Nothing was upserted into World KB or `spoke-memory`.

### 2. Replacement

There is none. References are source material, not a knowledge store of their own, and no runtime path promotes a reference body into World KB. Research-run output under `.nexus42/references/<run-id>/` is a different surface (run artifacts) and stays.

If a source-material registry is wanted again, it MUST be a new independent proposal (ideally an adapter that can feed the World KB extract gate). It MUST NOT revive this table, the `references/units/` tree, or the old routes as a compatibility shim.

## Invariants

None. The surface no longer exists; this page records only its retirement.

## Where it lives

- `crates/nexus-local-db/migrations/20261111000001_retire_user_and_reference_stores.sql` — the drop
- `.agents/adr/proposed/architecture/2026-11-10-retire-user-scope-and-references.md` — the authority (K1)

## Change obligations

Any change that reintroduces a reference or source-material registry replaces this page with the new proposal's design; it does not extend this one.
