# carry qc lessons to sibling plan — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/workflow-patterns/carry-qc-lessons-to-sibling-plan.md`](../../../knowledge/workflow-patterns/carry-qc-lessons-to-sibling-plan.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 22

```markdown
V1.156 shipped two sibling Canvas-layer plans that mirror each other: **P1** (World Timeline gains a Moment layer) and **P2** (Work Timeline gains a Brief layer). Both complete the 3×2 Brief/Narrative/Moment matrix; both are frontend-only projection extensions following the same adapter + orchestrator + inspector pattern (see `architecture-patterns/three-layer-timeline-projection.md`).

P1 ran first and its plan QC tri produced a fix-wave with four reusable findings:
- **W-1**: projected-layer nodes were selectable but the inspector dispatch fell through to the generic KB inspector → fired a guaranteed-failing `kb.patch_entity` write (PD-3 read-only violation).
- **W-2**: the alt-view/summarize path crashed on the new node kind (`block_type.localeCompare` on a node type without `block_type`).
- **F-3**: the fixture/data prop was missing from the projection `useMemo` deps → stale layer until a layer swap.
- **F-4**: lifting a `?layer=` URL restriction repurposed the old invalid-layer test slot → the null-branch went unpinned.
```

### at pre-freeze line 44

```markdown
In V1.156, P2 (the sibling) baked in all four P1 lessons proactively. P2's QC tri then found only **one** converged issue (a surface-specific graph-query status gate the lessons didn't cover) — a single fix-wave of one item, vs P1's fix-wave of four items including a PD-3 violation. The proactive carry-forward turned a likely 4-finding fix-wave into a 1-finding fix-wave and avoided a second PD-3 violation.
```

### at pre-freeze line 60

```markdown
## Examples

### V1.156 P2 brief — proactive P1 lessons

P2's T2 orchestrator brief included a section "Apply P1's QC lessons proactively (baked into the brief)" listing all four (read-only inspector, memo deps, alt-view crash-safety, invalid-layer test). The implementer applied them from the start. P2's QC tri (all three seats) verified the lessons were applied and found only the graph-query-status gap (surface-specific, not a pattern carry-forward).

P1 (no prior sibling) went through the full fix-wave discovery; P2 (with the carry-forward) needed one item.

```

### at pre-freeze line 70

```markdown
- V1.156 P1 QC consolidated `qc-consolidated.md` (the four lessons; local)
```
