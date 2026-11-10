# ADR: user layer entrance split — Historical Notes

Status: implemented — the retirement/dehistoricization this record documents is complete
Archived: 2026-10-13 — frozen retirement history; it no longer guides future work
Date: 2026-10-13
Scope: architecture
Source: migrated from the retired tracked specs corpus

## Problem

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

## Decision

The live specification states current behavior only. The historical material removed from it by the retirement pass is preserved verbatim below and is frozen: it is not current authority and receives no further edits.

## Alternatives considered

Not recorded in the ledger; the retirement pass preserved removed text instead of weighing competing alternatives.

## Consequences

**Provenance**: frozen from [`.agents/knowledge/architecture-patterns/user-layer-entrance-split.md`](../../../knowledge/architecture-patterns/user-layer-entrance-split.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

### Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

#### at pre-freeze line 31

```markdown
V1.170 P1 split the single Control Room SPA into two trees: **Create** (a reduced tree hiding operator chrome) and **Develop** (full config + Develop hub v1), selected by an entrance identity (`developer` | `content-creator`). The axis is deliberately **not** the Creator profile: Creator is the agent-layer identity aggregate (memory, world ownership, presets); entrance is the human's product-usage mode. v1 keeps entrance as web/desktop persisted client state only — the daemon never sees it (no User entity: a third first-party surface must not inherit a first-party persona).
```

#### at pre-freeze line 47

```markdown
#### 3. Enforcement-path rule (the lesson this doc exists for)
```

#### at pre-freeze line 49

```markdown
An axis that changes what is visible must be consumed by **every shell surface that renders visibility** — not just the route guard. In V1.170 P1 QC tri, the axis was wired at the route-guard + desktop-sidebar layer only, and **each** unwired surface became a Warning:
```

#### at pre-freeze line 95

```markdown
- Specs: `.agents/adr/archived/feature/2026-10-22-web-ui-retired-historical-notes.md`, `docs/specs/surfaces/desktop-shell.md`; iteration spec v1.170 (`v1.170-entrance-locks.md`) EL-1..EL-8 + AR-15..AR-22
```
