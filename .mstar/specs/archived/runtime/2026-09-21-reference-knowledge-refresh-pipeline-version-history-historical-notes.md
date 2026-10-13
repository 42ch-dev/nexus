# Reference Knowledge — Refresh Pipeline Version History — Historical Notes

**Provenance**: frozen from [`specs/active/runtime/reference-knowledge.md`](../../active/runtime/reference-knowledge.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 deleted the daemon host whose boot path spawned the refresh scheduler; the frozen lines span V1.26 static registration → V1.58 P1/P3 pipeline phases. Every normative rule the removed lines annotated is retained present-tense in the live spec; nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Status**: Normative (V1.58 P-last promote)
```

### at pre-freeze line ~5

```markdown
**Created**: 2026-06-22 (Draft at P1)
**Promoted to Master**: 2026-06-22 (V1.58 P-last)
```

### at pre-freeze line ~14 (§0 Document position)

```markdown
This Draft spec defines the `nexus.reference.refresh` capability and the
reference body refreshable scan pipeline introduced in V1.58 P1 (DF-44).
It covers the refresh policy model, DB schema for refresh tracking,
capability admission contracts, and the refresh-scheduler hook.

The static registration of reference sources was shipped in V1.26.
V1.58 P1 adds the refreshable pipeline core (capability + DB migration +
scheduler). V1.58 P3 adds the CLI subcommand and cross-cut E2E tests.
```

### at pre-freeze line ~36 (§1 Scope — Non-goals)

```markdown
Non-goals: CLI subcommand (`nexus42 reference refresh`) deferred to P3;
cross-cut E2E tests deferred to P3; `entity-scope-model.md` unchanged
(reference refresh is reference-source-scoped, not KB-scoped). **Durable roadmap:** DR-07.
```

### at pre-freeze line ~77 (§3 Refresh scheduler contract)

```markdown
has zero call sites in `crates/` and `apps/` (the daemon boot path that spawned
it was deleted with the v1.193 P2 host removal). Nor does any production caller
```

### at pre-freeze line ~84 (§3 Refresh scheduler contract)

```markdown
**V1.58 historical host:** the scheduler formerly lived in
`crates/nexus-daemon-runtime/src/refresh_scheduler.rs`, and the former
`crates/nexus-daemon-runtime/src/boot.rs` §4e spawned it at daemon startup.
Those deleted host paths describe the V1.58 composition, not current boot wiring.
The retained scheduler contract is:
```

### at pre-freeze line ~114 (§3 — Idempotency guard limitation)

```markdown
  CLI call (P3) arriving between the SELECT and the UPDATE, or two daemon
  instances against the same SQLite file, could both proceed to fetch. For the
  V1.58 single-daemon local model this is acceptable; a cross-process mutex or
  row-level optimistic concurrency check (OCC) on the refresh columns should be
  considered when P3 lands and multi-process scenarios are supported (DR-07).
```

### at pre-freeze line ~168 (§5 Sibling capability IDs (deferred))

```markdown
`nexus.reference.refresh_policy.get` and `nexus.reference.refresh_status` are
deferred to P3 if the user-facing surface (CLI) requires them. P1 ships only
`nexus.reference.refresh` as the core pipeline capability.
```

### at pre-freeze line ~189 (§6 Integration points)

```markdown
  Owns `spawn_refresh_scheduler` and `run_one_refresh_tick`; queries stale
  sources and dispatches refresh. The former V1.58 daemon boot composition is
  historical (§3), not a current startup anchor.
```

### at pre-freeze line ~233 (§7.2 Scheduled refresh flow)

```markdown
1. V1.58 historical daemon boot flow: the former host spawned the scheduler with a 60s initial delay. The current scheduler implementation is `crates/nexus-core/src/execution/schedules/refresh.rs` (§3); this example does not assert current host startup wiring.
```
