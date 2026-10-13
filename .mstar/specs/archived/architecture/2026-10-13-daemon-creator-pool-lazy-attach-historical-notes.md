# daemon creator pool lazy attach — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/daemon-creator-pool-lazy-attach.md`](../../../knowledge/architecture-patterns/daemon-creator-pool-lazy-attach.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 24

```markdown
The Nexus service boots without a creator SQLite pool when `active_creator_id` is absent from `~/.nexus42/config.toml`. This is **by design** (V1.118 AC-P0-1: boot-without-creator), not a bug. The Setup wizard's Continue path calls `ensureSetupBootstrap()` (an Electron IPC command) which **only writes** `active_creator_id` to config.toml — it does not notify the service or pre-open anything. A subsequent request that needs creator data must therefore resolve the workspace store on demand.
```

### at pre-freeze line 34

```markdown
## Historical (retired Rust daemon, deleted v1.193 P2)

The daemon-host pattern this doc originally recorded — a boot-held optional pool with `state.ensure_creator_pool().await` (idempotent lazy open + migrate) guarded by `state.pool_or_uninit()?`, and the "call `ensure_creator_pool().await` before pool access" rule for Tier-1 handlers — is **retired**. `ensure_creator_pool` and `pool_or_uninit` exist nowhere at HEAD; the `state`-owned pool and the Tier-1/Tier-2 middleware (`require_active_creator`) went with the daemon host. Keep the lesson (boot-without-creator is intentional; a write before bootstrap must fail closed), not the retired entry points.

```
