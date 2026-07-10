# Studio-First: Visual Then App (V1.105)

Reuse the V1.101–V1.103 discipline for wizard chrome.

## Workflow

1. **Studio fixtures** — portrait card + top horizontal Steps for Agent / Workspace / Done (include long agent-list scroll overflow).
2. **Visual acceptance** — light/dark; card height stable across all three steps; CTA row visible.
3. **App wiring** — only after (1)+(2), apply shell in `apps/web/src/pages/setup-wizard-page.tsx`.

## Per-plan Studio requirement

| Plan | Tier | Studio-first? |
|------|------|---------------|
| P0 Daemon fullscreen gate | Must | Optional (splash exists from V1.96/V1.97; polish only if needed) |
| P1 Wizard IA reorder | Must | Optional for step **content**; chrome owned by P2 |
| P2 Portrait shell + top Steps | Must | **Required** — block App Task 3 until fixtures accepted |

## Hard preferences

- `wire_contracts_changed: false`
- lucide only
- Do **not** reintroduce left step rail (`w-52` / 208px panel)
- Portrait geometry: ~480px wide × `min(720px, 85vh)` tall; content scrolls inside card
- Human desktop smoke is a **separate gate**; automated plan Done ≠ human smoke Done

## Acceptance checklist (P2)

- [ ] Fixtures cover Agent, Workspace, Done step states
- [ ] Fixtures include agent-list overflow (scroll inside card)
- [ ] Light + dark themes reviewed
- [ ] App shell matches accepted Studio chrome before plan close
