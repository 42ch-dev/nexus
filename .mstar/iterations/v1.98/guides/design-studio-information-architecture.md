# Design Studio — Information Architecture (V1.98)

**Status**: Draft (P-1 — product IA + architect technical notes)  
**Owner**: `@product-manager` (product framing) · `@architect` (technical alignment)  
**Consumers**: [`design-studio.md`](../../knowledge/specs/design-studio.md), P0 implement plan T2–T6  
**Normative DESIGN SSOT**: repo-root [`DESIGN.md`](../../../../DESIGN.md) + [`DESIGN.dark.md`](../../../../DESIGN.dark.md) (post-V1.98 merge)  
**Last updated**: 2026-07-08

---

## 1. Product intent

Design Studio is organized as a **flat top-nav gallery** — not a product shell with author workflows. Contributors scan left-to-right: foundations (Tokens) → brand (VI) → building blocks (Components) → language (Voice) → composition (Surfaces).

**Navigation principle:** every section is reachable in one click from the persistent header; no nested product routes beyond optional in-section anchors.

---

## 2. App chrome

| Element | Label | Behavior |
| --- | --- | --- |
| Product mark | **Nexus Design Studio** | Text + small `NexusMark`; links to Tokens (home) |
| Theme control | **Light** / **Dark** | Toggles `DESIGN.md` ↔ `DESIGN.dark.md` CSS layer; respects `prefers-color-scheme` on first paint |
| SSOT hint | **Read-only · edit `DESIGN.md`** | Persistent helper in header or footer; links to repo-root `DESIGN.md` — not an in-app editor or settings screen |
| Version strip | `DESIGN` version from frontmatter | Displays `version` field when parseable |

**Non-goals in chrome:** search, settings drawer, author profile, daemon status, workspace switcher.

---

## 3. Primary navigation

| Order | Nav label | Route slug | Priority | Purpose |
| --- | --- | --- | --- | --- |
| 1 | **Tokens** | `/tokens` | P0 | All scalar design scales from DESIGN frontmatter |
| 2 | **Brand** | `/brand` | P0 | `@42ch/nexus-ui` VI — logos, mark, theme.css |
| 3 | **Components** | `/components` | P0 | `apps/web` ui primitives — variant/state matrix |
| 4 | **Voice** | `/voice` | P1 | Voice & Content rules — typographic samples |
| 5 | **Surfaces** | `/surfaces` | P1 | Real chrome slices — Setup + App shell |

Default route `/` redirects to `/tokens`.

---

## 4. Section specifications

### 4.1 Tokens (`/tokens`)

**Sub-nav (in-page tabs or anchor list):**

| Sub-section | Label | Content |
| --- | --- | --- |
| Colors | **Colors** | Swatch grid: brand core, brand extended, neutrals, semantic accents; hex/rgba + token name |
| Typography | **Type** | Specimen rows for each `typography.*` key (heading, label, copy, button, mono) |
| Spacing | **Space** | Visual bar chart for `spacing.*` scale |
| Radius | **Radius** | Boxes demonstrating `rounded.*` tokens |
| Elevation / motion | **Motion** | Only if keys exist in merged SSOT; otherwise omit sub-nav (no placeholder) |

**Acceptance:** every color key in active theme frontmatter has a visible swatch; token name copyable (click-to-copy optional, not required V1.98).

### 4.2 Brand (`/brand`)

| Block | Label | Content |
| --- | --- | --- |
| Logo grid | **Logo variants** | `primary`, `color`, `white`, `mono` via `<NexusLogo>` with resolved asset paths |
| Mark | **Mark** | `<NexusMark>` at default + `currentColor` on light/dark panels |
| Theme CSS | **Theme variables** | `--nexus-brand-*` swatches from `@42ch/nexus-ui/theme.css` |
| Guidance | **Clear space** | `logoMinSizePx` + `logoClearSpaceRatio` from nexus-ui tokens; link to root DESIGN § Logo Usage |

### 4.3 Components (`/components`)

**Inventory (V1.98 base branch):** all primitive modules under `apps/web/src/components/ui/*.tsx` excluding `*.test.tsx`:

| Component | Gallery heading | Variants / states to show (minimum) |
| --- | --- | --- |
| `badge` | **Badge** | Default, secondary, destructive (if defined) |
| `button` | **Button** | primary, secondary, accent, ghost/outline; disabled; focus-visible |
| `card` | **Card** | Default card with header + content |
| `dialog` | **Dialog** | Open trigger + modal with title/description/actions |
| `input` | **Input** | Default, disabled, invalid/error |
| `label` | **Label** | Associated with input |
| `select` | **Select** | Closed + open list |
| `states` | **States** | Loading / empty / error affordances exported by module |
| `table` | **Table** | Header + 2–3 data rows |
| `tabs` | **Tabs** | Two tabs with panel content |
| `textarea` | **Textarea** | Default + disabled |

**Note:** `tabs` exists on disk; P0 T1 adds it to `components/ui/index.ts` barrel (see `design-unification.md` §7.2). Gallery scope = **11 primitive modules** listed above.

**Layout:** component name as `heading-20`; variant matrix in a bordered card; light and dark theme both visible (inherit global theme toggle).

### 4.4 Voice (`/voice`)

Fixture strings below are **canonical samples** from merged DESIGN § Voice & Content and shipped `apps/web` copy. Gallery renders them as labeled specimens — do not substitute marketing voice.

| Sample block | Label | Fixture string | Rule demonstrated |
| --- | --- | --- | --- |
| Page title | **Title Case** | `Welcome to Nexus` | Page titles, nav items, tabs, table headers |
| Helper text | **Sentence case** | `Nexus needs a workspace folder for your creative projects. We will create it if it does not exist.` | Helper text, empty states, descriptions |
| Primary action | **Verb + Noun** | `Create Work` · `Restart Daemon` · `Continue` | Actions name the object; avoid generic `OK` / `Submit` |
| Error toast | **Sentence case, object named** | `Preset validation failed. Fix the YAML errors and validate again.` | `What happened. What to do next.` — no protocol jargon |

Additional patterns (optional second row per block): empty state `No works yet. Create a Work to start the local loop.`; loading `Loading works…`; toast `Preset validated` (no trailing period).

### 4.5 Surfaces (`/surfaces`)

Studio-local fixtures compose `@web-ui/*` primitives with the placeholder strings below. No `apps/web` router, no daemon data.

#### Setup — Step card

| Element | Fixture copy |
| --- | --- |
| Step indicator labels | `Welcome` · `Daemon` · `Agent` · `Done` |
| Step body heading (Welcome) | `Welcome to Nexus` |
| Step body helper | `Nexus needs a workspace folder for your creative projects. We will create it if it does not exist.` |
| Inline row label | `Workspace location` |
| Inline row path (sample) | `~/Documents/nexus/default` |
| Browse affordance | `Browse…` |
| Primary CTA | `Continue` |

**Layout:** V1.96 integrated wizard card — vertical step list + Welcome step body inside one shared card chrome (`components.setup-wizard-surface` tokens). Static fixture only; no Tauri IPC.

#### App shell chrome

| Element | Fixture copy |
| --- | --- |
| Primary tabs | `Creator` · `Orchestrator` |
| Nested group (Creator tab) | `Works` → `All Works` |
| Nested group (Orchestrator tab) | `Runtime` → `Sessions` |
| Footer toolbar label | `Profiles` |
| Status strip (healthy sample) | `Daemon running` — helper: `Daemon API is reachable on the configured port.` |

**Layout:** sidebar tab strip + one expanded nav group + footer profile avatar row stub + slim daemon status strip — chrome only, no live routing or `NexusClient`.

---

## 5. Component matrix coverage checklist (P0 QA input)

Use as observable gate for Components section:

- [ ] 11/11 primitive modules listed in §4.3 render without runtime error
- [ ] Button shows dark-bg + light-text invariant (DESIGN contrast rule)
- [ ] Focus ring visible on at least one interactive control per theme
- [ ] Disabled states visually distinct from default
- [ ] Dialog traps focus when open (accessibility smoke)

---

## 6. Responsive behavior

- **Desktop-first** (≥1024px): full nav + multi-column token grids
- **Tablet (768–1023px):** nav may wrap; matrices stack single-column
- **Mobile:** not optimized — studio is contributor tooling; no mobile rewrite in V1.98

---

## 7. Relationship to `apps/web` IA

| `apps/web` (author product) | Design Studio |
| --- | --- |
| Control Room, Setup, Authoring routes | Gallery sections only |
| Daemon-backed data | Static fixtures |
| Sidebar two-tab IA (§29 web-ui) | Surfaces slice **references** chrome, does not duplicate full nav tree |

Authors never see studio nav labels in the shipped product.

---

## 8. Technical integration (architect)

### 8.1 CSS pipeline

Studio and `apps/web` share **`@nexus/design-tokens`** (`tooling/design-tokens`):

- `tokens.css` — `:root` + `.dark` CSS variable layers (extracted from web `index.css` in P0 T1)
- `tailwind.preset.ts` — single `theme.extend` source

Studio `src/index.css` imports the shared tokens layer, then `@tailwind` directives. Theme toggle adds/removes `.dark` on `<html>` (same class strategy as web `theme-provider`).

### 8.2 Component imports

Gallery **Components** and **Surfaces** sections import primitives via:

```
@web-ui/*  →  apps/web/src/components/ui/*
@web-lib/utils  →  apps/web/src/lib/utils.ts (cn only)
```

**Surfaces** (Setup card, shell chrome) compose `@web-ui/*` inside **studio-local fixture components** — do not import `apps/web/src/components/layout/*`.

### 8.3 Primitive count

**11 modules** (excluding `*.test.tsx`): badge, button, card, dialog, input, label, select, states, table, tabs, textarea. P0 T1 adds `tabs` to `components/ui/index.ts` barrel.

### 8.4 Dev port

Default **5174** to avoid collision with `apps/web` dev server (5173).
