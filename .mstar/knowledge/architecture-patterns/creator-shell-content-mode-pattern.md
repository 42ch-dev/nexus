---
module: apps/web + apps/design-studio
date: 2026-07-20
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-20-v1.128-p2-creator-create-controller-shell
tags: [creator-shell, layout, react-context, control-room, studio-first, presentational-extract]
applies_when: switching the Creator hub content region between empty-state Create CTAs and a selected-entity Controller surface without coupling to canvas routes or sidebar submenu anchor state
---

# Creator Shell Content Mode Pattern (Create page vs Controller stub)

**Track**: Knowledge (durable guidance, distilled from V1.128 P2 — Creator Create vs Controller shell).

## Context

Before V1.128, the Creator hub on `/works` and `/worlds` did not express the author mental model **Create → select entity → control**. Maintainers dogfooding Design Studio Shell fixtures could not preview the two modes, and App wiring risked overloading route params or sidebar `submenuItem` state — both wrong SSOTs for "which World/Work am I controlling in the hub content region."

V1.128 introduced a dedicated selection context and a presentational extract (`@web-layout/creator-shell-content`) with explicit **Back** semantics: clear selection and return to the Create page. Controller business widgets remain deferred — stub only.

## Guidance

### 1. Selection SSOT — `CreatorEntitySelectionContext`

Hub content mode is driven by React context, not URL params and not shell-sidebar submenu anchor state.

```ts
// apps/web/src/components/layout/creator-entity-selection-context.tsx
selectedEntity === null  → Create page
selectedEntity !== null  → Controller stub (work | world ref)
```

**Rules:**

- Provider lives in the layout layer (`CreatorEntitySelectionProvider`), wrapping routes that host the hub (`/works`, `/worlds`).
- `CreatorEntityRef` carries `{ kind: 'work' | 'world'; id; label }` — enough for stub copy and future Controller wiring.
- **Reject** using `submenuItem` from `shell-sidebar-chrome` as the mode switch — that state is for selection-submenu popover anchoring, not hub content IA.
- **Reject** encoding hub selection in `/works/:workId` or `/worlds/:worldId` route params — canvas/detail routes under those prefixes are **orthogonal** (per `work-shell-routes.ts`).

### 2. Presentational extract — `@web-layout/creator-shell-content`

Studio fixtures and App share one props-driven chrome module:

| `mode` | Renders | Host owns |
|--------|---------|-----------|
| `create` | Card-sized Create World / Create Work CTAs | `canCreateWorld` feature-detect, dialog open, navigation |
| `controller` | Placeholder copy + **Back** button | `selectedEntity`, `onBack` → `clearSelectedEntity` |

**Rules:**

- Extract stays in `apps/web/src/components/layout/presentational/` — no routing, no `NexusClient`, no internal selection state.
- Studio imports via `@web-layout/creator-shell-content`; App host (`creator-hub-page.tsx`) wires i18n labels and product actions.
- Create World CTA must honor V1.125 honesty: `hasCreateWorldClient(client)` — disabled + tooltip when absent; no silent no-op.

### 3. Back semantics (architect-locked)

**Back** on the Controller stub **clears** `selectedEntity` and returns the author to the **Create page** in the same hub route.

**Reject** the alternate "Back highlights a list row" pattern — hub content region and list selection chrome are separate concerns until a later iteration explicitly unifies them.

### 4. Studio-first verification

Ship Shell fixtures that toggle both modes before App wiring:

1. Empty → Create page with card CTAs (not list chrome).
2. Selected entity → Controller stub with TBD placeholder + Back.

App integration tests should cover at least one entity kind (Work); symmetric World-row coverage is a known nit residual (`R-V1128P2-001`).

## Why This Matters

- **IA clarity**: Authors see Create when nothing is selected and a distinct Controller surface when an entity is active — matching the long-term Control Room loop without prematurely building Controller business widgets.
- **Boundary hygiene**: Context SSOT prevents route-param hacks that would fight canvas deep links and work-shell navigation.
- **Studio parity**: Fixtures prove both modes before daemon/Tauri dogfood cost.

## When to Apply

- Extending Creator hub content (full Controller Panel widgets in a future iteration).
- Adding a third hub mode (e.g. multi-select) — extend context shape deliberately; do not overload submenu or route state.
- Any shell region that flips between empty-state CTAs and entity-scoped control chrome.

## Anti-patterns

- Using `useParams()` on hub list routes to infer Controller mode.
- Reusing `submenuItem` / selection-submenu anchor state as entity selection SSOT.
- Putting `useState` selection inside the presentational extract (breaks Studio fixture toggling and App/provider boundary).
- Shipping Controller business widgets in the same slice as the mode switch — stub + Back first; widgets when spec'd.

## References

- Iteration spec: `creator-create-controller-shell.md`
- Presentational extract: `apps/web/src/components/layout/presentational/creator-shell-content.tsx`
- Context: `apps/web/src/components/layout/creator-entity-selection-context.tsx`
- App host: `apps/web/src/pages/creator-hub-page.tsx`
- Related: [ui-component-promotion-workflow.md](./ui-component-promotion-workflow.md) (`@web-layout/*` alias tier)
