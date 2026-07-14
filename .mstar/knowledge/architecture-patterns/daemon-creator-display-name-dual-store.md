---
module: crates/nexus-daemon-runtime (creators handler) + apps/desktop/src-tauri (config.toml) + apps/web (footer/settings creators)
date: 2026-07-15
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-14-v1.117-profiles-workspace
tags: [creator, display_name, ssot, dual-store, identity-cache, sql, config-toml, daemon-api, drift]
applies_when: adding any write path for creator display_name / identity fields; reading creator name in a new UI surface; touching daemon creators handler
---

# Daemon Creator `display_name` Dual-Store SSOT

**Track**: Knowledge (durable guidance from V1.117 P0 QC1 F-001 — display_name drift).

## Context

The Nexus daemon stores creator `display_name` in **two independent stores** that
are not synchronized by any single write path:

| Store | Read by | Write path (pre-V1.117) |
| --- | --- | --- |
| **SQL `creators` table** (`display_name` column) | `GET /v1/daemon/creators` (`list_creators`) → `useCreators()` (footer, Settings lists) | Seeded at bootstrap / scan cache; **no UPDATE path** |
| **`~/.nexus42/creator_identity_cache.json`** (`creators.<id>.display_name`) | `GET /v1/daemon/creators/{id}` (`get_creator`), `GET .../active` | Updated by CLI login; **no daemon HTTP write path** |

The V1.117 P0 Setup Profile step added `PATCH /v1/daemon/creators/{id}` to let
authors edit their Profile display name. The first implementation wrote **only**
to the JSON identity cache. Because the footer reads the SQL store, the rename
was invisible in every creator-list surface — a silent SSOT drift (AC-P0-2
broken end-to-end).

## Guidance

**Any write to a creator identity field must keep both stores in sync.** Concretely,
`patch_creator` (and any future creator-display-name write) must:

1. UPSERT `display_name` into the SQL `creators` table (`UPDATE … WHERE creator_id`;
   `INSERT OR IGNORE` fallback if the row doesn't exist) — this is what
   `list_creators` / `useCreators()` read.
2. Update the `creator_identity_cache.json` entry — this is what `get_creator` /
   `get_active_creator` read.

Do **not** assume one store derives from the other. The V1.94-era split was never
reconciled; picking either store alone as SSOT leaves the other stale.

## Why This Matters

The two stores serve different read paths with no shared write authority. A
single-sided write produces a **non-deterministic display name** across the
product (Setup shows the new name; the footer shows the old SQL name) until an
unrelated scan/seed happens to refresh the lagging store. This class of bug is
invisible to unit tests that only assert on one store.

## When to Apply

- Adding any new creator field write (display_name, handle, avatar, …).
- Reading creator name in a new UI surface — confirm **which** store that surface
  reads, and whether a write you depend on updates it.
- Migrating the daemon creators handler to generated contract types (residual
  `R-V1117P0QC1-F002`): the generated `CreatorDetail`/`CreatorInfo` shapes hide
  the SQL↔JSON split — the sync requirement must be preserved in the handler.

## What Didn't Work (V1.117 P0)

- **PATCH → JSON cache only:** display_name set in Setup did not appear in the
  footer (`useCreators()` reads SQL). Caught by QC1 F-001.
- **`load_identity_cache` returning `Null` on corruption → writing a fresh empty
  cache:** a corrupt cache file was silently wiped on the next PATCH (QC2 F-002).
  The fix distinguishes "file absent" (init fresh) from "file corrupt" (error,
  do not overwrite) + atomic write (temp+rename).

## Examples

Correct write (post-V1.117 P0 fix-wave, `patch_creator` in `creators.rs`):

```rust
// 1. SQL store (read by list_creators / useCreators)
sqlx::query!("UPDATE creators SET display_name = ?, cached_at = ? WHERE creator_id = ?", …);
// INSERT OR IGNORE fallback when 0 rows affected

// 2. JSON identity cache (read by get_creator / get_active_creator)
// atomic write to ~/.nexus42/creator_identity_cache.json
```

## Related

- `architecture-patterns/acp-registry-id-matching.md` — agent identity matching by
  `registry_agent_id` (stable) vs `name` (mutable); a related "match by stable id,
  not label" lesson in the agent domain.
- Residual `R-V1117P0QC1-F002` — migrate daemon creator handlers to generated
  `@42ch/nexus-contracts` types (the hand-written `CreatorDetail`/`PatchCreatorRequest`
  are duplicates of existing generated shapes).
