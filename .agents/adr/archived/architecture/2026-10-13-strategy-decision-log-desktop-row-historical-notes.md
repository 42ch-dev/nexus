# ADR: STRATEGY Decision Log — desktop-row path reference — Historical Notes

Status: implemented — the retirement/dehistoricization this record documents is complete
Archived: 2026-10-13 — frozen retirement history; it no longer guides future work
Date: 2026-10-13
Scope: architecture
Source: migrated from the retired tracked specs corpus

## Problem

The Decision Log row in `STRATEGY.md` named the retired `apps/desktop` tree in a parenthetical. The dead path reference was dropped in place (the row's decision content is unchanged); its pre-freeze wording appears here verbatim. Nothing here is current authority.

## Decision

The live specification states current behavior only. The historical material removed from it by the retirement pass is preserved verbatim below and is frozen: it is not current authority and receives no further edits.

## Alternatives considered

Not recorded in the ledger; the retirement pass preserved removed text instead of weighing competing alternatives.

## Consequences

**Provenance**: frozen from [`STRATEGY.md`](../../../../STRATEGY.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed `apps/desktop/**` path was retired with the native desktop composition, and no single uncontested calendar date is asserted here, so the freeze date is used).
**Freeze date**: 2026-10-13

---

### Removed-line ledger (verbatim, in original order)

The Decision Log row in `STRATEGY.md` named the retired `apps/desktop` tree in a parenthetical. The dead path reference was dropped in place (the row's decision content is unchanged); its pre-freeze wording appears here verbatim. Nothing here is current authority.

#### at pre-freeze line 106 (Decision Log — desktop shell row)

```markdown
| Desktop shell (`apps/desktop`) via a native host | Wraps the web SPA for native OS capabilities (file open, reveal in Finder, sidecar lifecycle); no second DTO set | V1.66 (June 2026) |
```
