# Schemas Directory Layout — Rename History Historical Notes

**Provenance**: frozen from [`specs/active/architecture/schemas-directory-layout.md`](../../active/architecture/schemas-directory-layout.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-04 — the removed "Last updated" line's own date ("2026-09-04 — reconciled current Daemon API schema and generated-module names through V1.183"); the removed blocks are the file's self-declared rename history and version stamps. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## §5 Historical renames (removed table)

| Old path | Current | Done |
| --- | --- | --- |
| `schemas/cli-sync/` | `schemas/cloud-sync/` | 2026-05-20 — `$id` URIs updated |
| `schemas/acp-runtime/` | `src/local/acp_runtime/` | V1.4 WS5 |
| `schemas/meta/` | `src/local/meta.rs` | V1.4 WS5; directory removed V1.21 |
| `schemas/cloud-sync/` | `schemas/platform/sync/` | 2026-06-23 (V1.62 P0) — folded into consumer-scope `platform/sync/`; `bundle.schema.json` renamed `bundle-refinement.schema.json` |
| `schemas/compute/` | `schemas/daemon-api/compute/` (+ entity-* deleted) | 2026-06-23 (V1.62 P0) moved first to historical `schemas/local-api/compute/`; the later Daemon API rename produced the current path |
| `schemas/local-api/` | `schemas/daemon-api/` | Daemon API namespace rename; current generated modules are Rust `generated::daemon_api` and TypeScript `generated/daemon-api` |
| `schemas/domain/{bundle,delta,sync-command}` | `schemas/platform/sync/` | 2026-06-23 (V1.62 P0) — sync payloads, not wire entities |
| `schemas/platform/*.schema.json` (flat) | `schemas/platform/http-bff/*.schema.json` | 2026-06-23 (V1.62 P0) — consumer-scope split into `http-bff/` + `sync/` |

## Annotation drops (reworded in place; originals in git history)

- §0 Status row: "**V1.139 architect §5.2**: §3.3 domain/ table — `key-block.schema.json` **deleted** (V1.139 SPOKE adoption; KB entry type now sourced from spoke …)" → present-tense statement kept, version provenance dropped.
- §0 Last updated: "2026-09-04 — reconciled … through V1.183."
- §1 heading "(2026-09, current through V1.183)"; §1 removed-path date/version clauses ("renamed `cloud-sync/` (2026-05-20)", "(2026-06-23, V1.62 P0)" ×3, "(V1.62 P1)").
- §2 header "(post-v1.193 P2)"; §2 ownership note "(v1.193 P2)" + "The `nexus-daemon-runtime` handlers named in this column before v1.193 were deleted with the crate in v1.193 P2; serving ownership moved to …" → present-tense.
- §3.1 "(V1.20 removed world/explore **daemon** routes; platform HTTP contracts **remain** wire here)".
- §3.2 "moved here from `domain/` (V1.62 P0)".
- §3.3 key-block row "**DELETED (V1.139).**"; trailing "(moved to `platform/sync/` in V1.62 P0)".
- §3.4 "Removed `schemas/meta/` (V1.4 WS5 + V1.21 cleanup)".
- §4 "(see §1 + §5)" → "(see §1)"; "declare in `modules/<id>/manifest.json` (V1.62 P1)"; "**Re-audit:** add a dated appendix under §5 when moving or renaming folders." → audit-note pointer (§5 renames table frozen above).
- §7 heading "(2026-09, current through V1.183)"; "`domain/` | 10 → **9** (V1.139: `key-block.schema.json` deleted)"; "and notable V1.64 deltas".
