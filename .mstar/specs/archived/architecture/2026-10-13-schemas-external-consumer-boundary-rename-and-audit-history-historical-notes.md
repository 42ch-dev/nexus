# Schemas — External-Consumer Boundary — Rename and Audit History — Historical Notes

**Provenance**: frozen from [`.mstar/specs/active/architecture/schemas-external-consumer-boundary.md`](../../active/architecture/schemas-external-consumer-boundary.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text names the V1.20/V1.62/V1.64/V1.183 iterations without a calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order. Still-true statements were reworded in place in the live spec; their superseded wording appears here. Nothing here is current authority.

```markdown
**Status**: Active — current external daemon contracts use the Daemon API namespace; V1.64 originally established the bundled Web UI as an external API consumer
**Last reconciled**: 2026-09-04 — current schema tree, generated module names, and Daemon API namespace through V1.183.
## What still lives in `schemas/` today (2026-09, current through V1.183)
| `schemas/platform/http-bff/*` | **Yes** — `nexus-platform` | Platform HTTP request/response contracts (was flat `schemas/platform/*` pre-V1.62) |
| *(removed from `schemas/`)* | **No** | `cli-sync/` (→ `cloud-sync/` → `platform/sync/`), `acp-runtime/`, `meta/`, `cloud-sync/`, `compute/` (entity-attributes/entity-state → `modules/<id>/manifest.json` in P1), `outbox_entry`, daemon/orchestration types → `src/local/` |
V1.20 removed **daemon local HTTP proxies** for `world/*` and `explore/*`; those operations use **platform HTTP** directly. The `schemas/platform/http-bff/world-*` and `.../explore-*` files remain **wire** for platform — they were never "daemon-only" contracts.
V1.62 reorganized `schemas/` along consumer-scope lines and first moved compute envelopes to the historical `local-api/compute/` tree. The later Daemon API namespace rename moved all such external contracts to the current `schemas/daemon-api/` tree. Per-module entity shape schemas remain deleted; module-local shapes live in `modules/<id>/manifest.json`.
- **Stale path risk**: do not reference `schemas/cli-sync/`, `schemas/meta/`, `schemas/acp-runtime/`, `schemas/cloud-sync/`, or `schemas/compute/` — removed or renamed (see layout spec §1 + §5 historical renames).
- **Historical audit (pre-rename paths)**: retired `schemas-boundary.md` §5.2 — 53 wire / 10 local at audit time (local process record). Re-run an equivalent source scan before further moves; search `<TypeName>` in `nexus-platform` before deleting generated TypeScript.
*Created: 2026-05-20 (as `schemas-wire-platform-sync-boundary.md`). Renamed + scope expanded 2026-06-23 (V1.62 P0). V1.64 expands external Local API consumer scope to the bundled local Web UI and records the strict handler DTO drift-closure criterion. Pointer doc; do not duplicate the archived audit table here.*
```
