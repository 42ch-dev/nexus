# P2 — Light Mode interactive VI (V1.136)

**Status:** draft (Phase 1 §1.6 — **Architect §5.2 technical contract locked**; writing-specialist §5.3 complete — PM lock next)  
**Plan:** `2026-07-23-v1.136-p2-light-mode-chrome-harmony`

## Author problem (plain language)

In **Light Mode**, bright neon cyan (`#25D1E0`) on buttons and selections feels **刺眼 and out of place** — that color belongs in **Dark Mode** only. Using deep office blue (`#0D2B3E`) as the primary button fill feels too **corporate**, not Chronos. I want a calmer **mid-teal** (`#117480`, `brand-cyan-1000`) for interactive fills with white label text, while keeping deep blue for the titlebar, logo, and text links. Error retry buttons inside red alert blocks should be **small text links**, not big filled buttons.

## User value

- **Harmony:** Light Mode reads as Chronos teal — calm, branded, not neon or office-generic.
- **Consistency:** One Button/token SSOT in Studio; product shell and canvas consume the same interactive token.
- **Calm errors:** Transport failures offer actions without shouting over the alert.

## Intent

Unify Light Mode **interactive** highlight to Chronos mid-teal **`brand-cyan-1000` (`#117480`)** + white on fills. Neon cyan **Dark-only** for primary signal/CTA. Ink `brand-deep-blue` for structure and text links. `TransportErrorBlock` CTAs = compact text links. Studio Tokens/Brand/Components/Surfaces = author SSOT; product reuses.

## Token lock (product)

| Role | Light | Dark |
|------|-------|------|
| Primary Button fill | `brand-cyan-1000` + white label | `brand-cyan` + deep-blue label |
| Selection / canvas accents / spinners / active chrome | `brand-cyan-1000` | `blue-700` / neon cyan |
| Text links / titlebar / logo ink | `brand-deep-blue` | unchanged |
| Neon cyan `#25D1E0` | **not** light interactive default | primary signal / CTA |

## Architect technical contract (§5.2 — normative)

### Architecture decisions (PM open questions)

| # | Question | Decision |
|---|----------|----------|
| **Q5** | P2 wave vs P1 fixture token ordering | **P2 wave 2** lands token/Button SSOT. P1 fixture uses semantic `Button`/`HubTabBar` — **no P1 blocker**. If P1 merges before P2 on integration branch, P1 fixture may briefly show deep-blue primary until P2 merges; QC accepts. P2 T4 converts any touched dual-theme fixtures. |
| **Q6** | Explicit file/token retarget map | See tables below — grep-driven closure required in P2 T4. |

### Token alias map (light interactive lock)

| Semantic role | Light token / class | Dark token / class | SSOT file |
|---------------|---------------------|--------------------|-----------|
| Primary Button fill | `bg-brand-cyan-1000` (`#117480`) + `text-brand-white` | `bg-brand-cyan` + `text-brand-deep-blue` | `packages/nexus-ui/src/components/button.tsx` |
| Primary Button hover/active | `brand-cyan-1000` scale (`blue-900`/`blue-1000` aliases in light) | `blue-800`/`blue-900` cyan steps | `tooling/design-tokens/src/tokens.css` |
| Selection border / active bar / focus ring outer | `blue-1000` (light) → `#117480` | `blue-700` neon cyan | `tokens.css` + `DESIGN.md` component tokens |
| Spinner / wizard step active / splash accents | `text-blue-1000` / `var(--color-blue-1000)` | `text-blue-700` | `tokens.css` `--color-setup-wizard-*`, `--color-footer-profile-*` |
| Canvas selected node / timeline / narrative accent | `--color-canvas-*` → light uses `blue-1000` not `blue-700` | unchanged neon | `tokens.css` canvas block |
| Text links / titlebar / logo ink | `text-brand-deep-blue` | `dark:text-blue-700` | explicit — **never** light `text-blue-700` for links |
| Neon cyan `#25D1E0` / light `blue-700` | **Forbidden** as light interactive fill/selection default | primary signal / CTA | DESIGN.md §Brand Colors |

**Implementation strategy:** Retarget **light-theme values** in `tokens.css` for interactive aliases (`--color-blue-700` used as interactive signal in component tokens → split: light interactive paths reference `blue-1000`, dark keep `blue-700`). Update `DESIGN.md` VI-002 primary button rule. Do **not** rename CSS var keys.

### `TransportErrorBlock` primitive contract

| Field | Value |
|-------|-------|
| **CTA control** | `<button type="button">` styled as compact text link — **not** `Button` variant primary/tertiary |
| **Typography** | `text-label-14 font-medium` |
| **Colors** | light: `text-brand-deep-blue`; dark: `text-blue-700` |
| **Layout** | Single row `flex flex-wrap gap-x-4 gap-y-1` under message; `mt-2` max |
| **testids** | Keep `transport-error-primary`, `transport-error-secondary` |
| **Owner** | `packages/nexus-ui/src/components/transport-error-block.tsx` |

### File retarget map (grep closure — P2 T4)

**Tier 1 — SSOT (change once):**

| File | Retarget |
|------|----------|
| `tooling/design-tokens/src/tokens.css` | Light interactive aliases: canvas, wizard, footer, spinner tokens → `blue-1000` |
| `DESIGN.md` | VI-002 primary button light = cyan-1000; update contrast tables |
| `packages/nexus-ui/src/components/button.tsx` | Light primary → `bg-brand-cyan-1000 text-brand-white` + hover/active scale |
| `packages/nexus-ui/src/components/transport-error-block.tsx` | Link CTAs per contract above |
| `packages/nexus-ui/src/components/button.test.tsx` | Assert light primary cyan-1000 |

**Tier 2 — Product bypass sites (retarget or remove local override):**

| File | Issue | Action |
|------|-------|--------|
| `shell-sidebar-chrome.tsx` | `bg-brand-cyan` creator tab pill in light | Use theme-split class aligned with Button SSOT |
| `hub-tab-bar.tsx` | `after:bg-brand-cyan` active indicator | Light → `after:bg-brand-cyan-1000` |
| `work-timeline-canvas.tsx` | `bg-brand-cyan` CTA button | Use `Button variant="primary"` |
| `agent-picker.tsx` | `border-blue-700` selection (OK as signal) | Light selection ring → `border-blue-1000` |
| `apps/web/src/components/ui/states.tsx` | Spinner `text-blue-700` | Light → `text-blue-1000` |
| Canvas / outline components using `border-blue-700` on light inputs | Hard-coded bypass | Prefer token vars or `blue-1000` |

**Tier 3 — Studio proof (single-theme follow toggle):**

| Gallery | Fixture |
|---------|---------|
| Tokens | cyan-1000 interactive swatch visible |
| Brand | logo unchanged (ink deep-blue) |
| Components | Button matrix + TransportErrorBlock matrix |
| Surfaces | shell/wizard/splash/Work Timeline Selected |

**Grep commands (closure evidence):**

```bash
rg 'bg-brand-cyan|bg-brand-deep-blue' apps/web packages/nexus-ui --glob '*.tsx'  # audit bypasses
rg 'text-blue-700' apps/web --glob '*.tsx'  # light link misuse smell
rg 'after:bg-brand-cyan' apps/web  # hub tab indicator
```

### P2 ↔ P1 dependency

| Rule | Detail |
|------|--------|
| Wave order | P2 **wave 2** after P0/P1 wave 1 (compass) |
| P1 not blocked | P1 ships semantic primitives; P2 retargets tokens underneath |
| Shared files | If P1 touches `hub-tab-bar.tsx`, P2 owns light indicator retarget in same file — coordinate on integration branch merge order |

## Product acceptance (P2G — PM locked)

Author-observable gates. Plan AC-1–AC-6 map 1:1 below.

| ID | Author can observe… | Pass | Fail |
|----|---------------------|------|------|
| **P2G-1** | Components Button (light) | Primary fill = `brand-cyan-1000` + white on Studio Components matrix | Neon cyan or office deep-blue primary fill in light |
| **P2G-2** | Light canvas / shell accents | `canvas-node-border-selected`, timeline/narrative accents, wizard/splash/spinner use cyan-1000 in light | Light neon cyan selection; hard-coded `blue-700` bypass |
| **P2G-3** | TransportErrorBlock | Primary + secondary actions = **compact text links** (ErrorState-aligned); full matrix in Studio | Filled Button inside alert; oversized CTA |
| **P2G-4** | Theme split | Light shell/wizard/splash use cyan-1000 interactive; dark keeps neon cyan CTA | Neon cyan as light interactive default |
| **P2G-5** | SSOT discipline | No parallel product VI; DESIGN + tokens greppable; touched Surfaces = single-theme-follow-toggle | One-off hex in `apps/web`; dual Light\|Dark matrices |

**Compass mapping:** AC-I3, AC-I4, AC-I5, AC-I6, AC-I7, AC-I8 ↔ P2G-1–P2G-5.

## TransportErrorBlock presentation lock (product)

- Both primary + secondary CTAs: compact **text link** controls (not filled `Button`).
- `text-label-14`; light link color = `text-brand-deep-blue`; dark = cyan link token.
- Single compact row under message; must not dominate the red alert block.

## Anti-patterns (do not ship)

1. **One-off hex in apps/web** — all interactive retargets flow from tokens + `@42ch/nexus-ui`.
2. **Light neon cyan selection** — `#25D1E0` / `blue-700` as light interactive default.
3. **Office deep-blue primary fill** — superseded by cyan-1000 for interactive fills (deep-blue remains ink/links).
4. **Parallel Button in product** — second button styling in shell bypassing Components SSOT.
5. **Dual-theme Studio matrices** — side-by-side Light\|Dark frames on touched surfaces.

## Non-goals

- Dark primary CTA redesign (neon cyan stays)
- Replacing titlebar ink deep-blue
- Inventing a second Button primitive in `apps/web`
- Changing error **message** copy or alert severity colors

## PM sign-off (§5.1)

| Field | Value |
|-------|-------|
| **Product intent** | Ready for Architect §5.2 |
| **Date** | 2026-07-23 |
| **Blocked** | None — clarify closed via author Studio Components feedback |

## Architect sign-off (§5.2)

| Field | Value |
|-------|-------|
| **Technical contract** | Locked — token alias map, TransportError link primitive, file retarget map |
| **Date** | 2026-07-23 |
| **PM Q5–Q6** | Answered in Architecture decisions table above |
