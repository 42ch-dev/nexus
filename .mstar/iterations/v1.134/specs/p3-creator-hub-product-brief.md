# Product brief: Creator Hub dual-pane IA (V1.134 P3)

**plan_id:** `2026-07-23-v1.134-p3-creator-hub-dual-pane-ia`  
**iteration:** V1.134  
**Status:** product intent (Seat 1) — architect expands full IA + selection/tab-link contract in Prepare  
**Authority:** product framing only; normative technical contract → architect §5.2 (`specs/p3-creator-hub-dual-pane-ia.md` or successor path under this package)

## Problem

The Creator Hub is the author's primary creation surface, but today's IA fights the mental model:

| Today | Author expects |
|-------|----------------|
| Create via **dialogs/modals** | Create **inline** in the left workspace |
| Left = Create-only strip (V1.132) | Left = **usable 功能区** (tabs + create/select) |
| Right = lists; selection opens a **full-page controller stub** | Right = **card list** that stays; dual-pane chrome is stable |
| Entity kind not a linked dual-pane tab pair | **世界 / 作品** tabs on **both** panes, always in sync |
| Empty list is mute / unclear | Empty state steers: create from the left |

This is a **hardening / IA correction**, not a new product line.

## Target user

Local-first authors dogfooding Nexus desktop/web Control Room — especially first create and daily World/Work browsing.

## User stories

1. **As an author opening 创作**, I see a **left workspace + right content** layout immediately — not a modal ceremony and not a controller stub page.
2. **As an author with no Worlds (or no Works)**, the right pane tells me clearly there is no content yet and that I should create from the left (i18n: zh-CN intent *无内容，从左边创建*; en catalog equivalent required).
3. **As an author switching 世界 ↔ 作品**, both panes switch together — I never see left on Worlds and right on Works.
4. **As an author creating a World or Work**, I complete create **inline in the left pane** without a hub modal.
5. **As an author opening an existing entity**, I do **not** lose the dual-pane list layout to a full-page controller stub; navigation to canvas (if applicable) uses existing orthogonal routes.

## Product rules (locked for architect)

1. **Dual-pane is stable chrome.** Left workspace + right card list remain the hub frame. Do not reintroduce full-page controller-stub replace on selection.
2. **Left = workspace 功能区.** Inline create + select affordances for the active entity kind. Not modal-first on the hub.
3. **Right = card list** for the active kind only (World cards **or** Work cards, not mixed).
4. **Linked tabs.** World/Work (世界/作品) controls appear in a way the author reads as **both panes**; one shared tab SSOT; switching is bidirectional and consistent.
5. **Canvas orthogonality.** `/works/:workId/*` and `/worlds/:worldId/*` stay product-orthogonal; hub does not absorb canvas.
6. **Studio-first.** Dual-pane fixture accepted by the **author** in `apps/design-studio` (World + Work × empty + populated × light + dark) before `apps/web` wiring claims.
7. **i18n.** New user-facing strings (empty state, tab labels if new, inline create labels) ship en + zh-CN from day one.

## Acceptance (maps to compass AC-12…AC-17)

See [delivery-compass.md](../delivery-compass.md) § Acceptance Criteria · P3. Plan DoD must map tasks → those ACs.

## Non-goals (product)

- Entity Chat deepening, Orchestrator 功能区 redesign, Fork UI, timeline/canvas feature work
- Rewriting shell footer / 工作区 parent / 创作|编排 mode switch
- Package promotion of hub chrome into `@42ch/nexus-ui` unless architect explicitly scopes it
- Wire-contract / new daemon create APIs (reuse existing create endpoints)
- Stretch animation / density experiments

## Relationship to prior IA

- **V1.130/V1.132 shell:** dual-pane shell + Create-only left for hub was correct for "no left Menu of Worlds/Works."  
- **V1.134 P3:** evolves **hub content** so left is a real workspace (tabs + inline create) and right remains the list — still **no left Menu dump of the full library as primary nav**. Lists stay on the right.
- Architect must reconcile with `workspace-parent-shell-ia` / creator-shell patterns without reopening shell footer invariants.

## Open for architect (do not invent in product seat)

- Exact tab control placement (one shared control vs mirrored controls that stay linked)
- Selection → navigate vs in-pane detail (must preserve stable dual-pane chrome)
- Which create dialogs become inline vs remain for non-hub callers
- Component ownership (`creator-hub-page`, lists panel, new left pane, selection context extensions)
- Whether any presentational extract needs Studio `@web-*` alias vs app-local only
