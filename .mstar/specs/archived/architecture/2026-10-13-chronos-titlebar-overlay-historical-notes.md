# chronos titlebar overlay — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/chronos-titlebar-overlay.md`](../../../knowledge/architecture-patterns/chronos-titlebar-overlay.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 20

```markdown
**Track**: Knowledge (durable guidance, distilled from V1.131 P0 Chronos titlebar chrome).
```

### at pre-freeze line 28

```markdown
### 1. Desktop-host Overlay — keep decorations (historical: the native v2 host, retired v1.192)
```

### at pre-freeze line 30

```markdown
> **Historical:** the native v2 desktop host was retired in v1.192; Electron (`apps/desktop-electron`) is the only desktop composition now. The host-specific settings below are retained as a historical record; the web-side drag-region attribute contract was retired in v1.214 — the attribute was removed as dead (no consumer). What the web titlebar retains is the plain ink chrome, the macOS safe inset, and the empty-paint double-click callback.

| Setting | Value |
| --- | --- |
| `titleBarStyle` | `"Overlay"` |
| `hiddenTitle` | `true` |
| Decorations | **on** (native traffic lights) |
| `trafficLightPosition` | Align native controls with the Chronos bar height/inset |

The **web** titlebar paints the ink plane under the native chrome. Logo/title paint and the empty spacer are plain ink chrome; the drag-region attribute was retired in v1.214 (removed as dead — no consumer), and what remains is the macOS safe inset plus the empty-paint double-click callback. Gear, theme, and health controls are explicit interactive islands.
```

### at pre-freeze line 58

```markdown
Do **not** mark Overlay live-smoke residuals resolved without human evidence for **H2–H6 + D1–D3**. Product code can still Profile-B Done with an open residual and the durable V1.132 protocol (`titlebar-window-drag-overlay-smoke.md`).
```

### at pre-freeze line 63

```markdown
- Claiming Dock/titlebar live visuals from compose/preview alone (same class of error as premature `R-VI-003` archive).
```

### at pre-freeze line 67

```markdown
- Spec (V1.132): `titlebar-window-drag.md`
- Smoke protocol (V1.132): `titlebar-window-drag-overlay-smoke.md`
- V1.131 baseline: `chronos-titlebar-chrome.md`
```
