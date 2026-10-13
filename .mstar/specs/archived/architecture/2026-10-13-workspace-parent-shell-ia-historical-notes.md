# workspace parent shell ia — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/workspace-parent-shell-ia.md`](../../../knowledge/architecture-patterns/workspace-parent-shell-ia.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 22

```markdown
**Track:** Knowledge. Distilled from V1.132 P3; **V1.135 P0** locks hub create in the **sidebar** (`panelContent`) and hub **content** as browse-only; **V1.136 P1** replaces dashed create cards with **inline-tab create** in the same slot. Still supersedes V1.131 orchestrator-only 工作区 and V1.130 creator-left-Menu.
```

### at pre-freeze line 24

```markdown
## V1.134 P3 correction (superseded)
```

### at pre-freeze line 26

```markdown
**V1.134 P3** placed hub create in the **content dual-pane left column** (`HubWorkspacePane` / `CreatorHubDualPane` inline create) and hid sidebar `panelContent` on `/works` and `/worlds`. That placement is **incorrect for author intent** — authors expect create in the persistent **sidebar / 功能区** shell column, not inside the main content pane. **V1.135 P0** inverts it; do not cite V1.134 P3 dual-pane-left-create as the normative pattern or keep it as a compatibility branch.

## V1.136 P1 — inline-tab create (normative)

**V1.135 P0** restored sidebar placement but still used two dashed **`CreateCardButton`** cards that opened dialogs. **V1.136 P1** replaces that with an **inline create zone** in the same `panelContent` slot:
```

### at pre-freeze line 83

```markdown
7. Do not reinstate V1.131 AC-4, V1.130 left-Menu, V1.132 Create-only-left + controller-stub, **V1.134 P3 content dual-pane left-create**, or **V1.135 card-button hub create** as compatibility branches.
```

### at pre-freeze line 90

```markdown
- **Empty sidebar on hub while create sits in content-left** → restore `panelContent` on hub; collapse hub to browse-only (regression of V1.134 P3 placement).
```
