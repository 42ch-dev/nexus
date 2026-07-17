---
iteration_id: V1.121
start_date: 2026-07-17
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.121
plans:
  - 2026-07-17-v1.121-design-language-foundation
  - 2026-07-17-v1.121-component-library-elevation
  - 2026-07-17-v1.121-app-surfaces-elevation
  - 2026-07-17-v1.121-canvas-reading-elevation
---

# V1.121 Delivery Compass — The Literary Engine: design system elevation

> **Phase 1:** product-manager §5.1 **done** · architect §5.2 **done** · writing-specialist §5.3 **done** (seat 3 returned empty report; package verified on disk by PM — knowledge clean, index accurate, prose coherent).
> **PM lock (§5.4):** `status: locked` (2026-07-17). Prepare gates pass on all four plans (specify / clarify / plan). Spec freeze locked.
>
> **Direction lock mode: autonomous** (`/iteration-loop`, scale **L** — 4 business plans).
> Locked direction, rationale, and candidate trade-offs are recorded below per `mstar-iteration` §1.2 autonomous.

## Autonomous direction lock record

**Caller constraint (direction arg):** 全面 review 并升级系统的前端设计系统 — explore latent problems, fix and optimize; raise premium feel / design quality; theme keywords **创作 / 文学 / AI / 互联 / 画布 / 计算引擎**; adopt the upgrade across **design-studio, apps/web, and the UI component library**; verify correctness. Scale **L+** → 4 business plans (L cap).

**Candidates evaluated (research: DESIGN.md v0.3.0, tooling/design-tokens, apps/web, apps/design-studio, specs, V1.94/V1.106/V1.111/V1.113 design iterations):**

| # | Candidate | Trade-off | Verdict |
|---|-----------|-----------|---------|
| A | **The Literary Engine** — full design-language elevation (typography voice, ink atmosphere, elevation/motion depth, canvas ambient, token hygiene) applied to studio + web + component library | Largest blast radius; needs staged token-first rollout and dark/light AA re-verification | **LOCKED** — matches caller direction exactly; evidence shows concrete debt (below) |
| B | Canvas-only visual identity pass | Too narrow; ignores shell/components/studio the caller named | Rejected (scope mismatch) |
| C | Component-library promotion + token hygiene only (engineering pass) | No aesthetic elevation; misses 高级感 intent | Rejected (insufficient ambition) |
| D | Dark-theme-only refinement | Single-axis; leaves typography/canvas/studio gaps | Rejected (partial) |

**Evidence base for A (from Phase 1 research):**

- Typography is 100% system sans; the only serif is a hardcoded `Georgia` in reading chrome — no literary voice for a creative-writing product (`apps/web/src/index.css` `@layer components`).
- Dark theme is a mechanical neutral flip (`#0a0a0a/#111/#1a1a`) with zero brand atmosphere; ink-blue brand depth unused.
- Elevation is 3 flat shadows; no hover depth, no layered ambient — surfaces read flat, not premium.
- Canvas token layer carries Tailwind-palette leftovers (`#94A3B8`, `#3B82F6`, `#10B981`, `#F59E0B`, `#A78BFA`, `#0EA5E9`, `#EF4444`, `#8B5CF6`, `#EDE9FE`) outside the brand semantic scales — chromatic drift on the signature surface.
- ~40–50 arbitrary Tailwind values across pages/components; `dialog.tsx` overlay hardcodes `bg-black/40` despite the V1.111 `scrim` token; reading-chrome block has 40+ untokenized values.
- Design-studio has no Typography / Spacing / Radius / Elevation / Motion / Canvas-token galleries — the design system is partially invisible to its own verification harness.
- `tailwind-merge` registers only V1.94 font-size tokens; any new scale needs registration or silent class-stripping returns.

**Locked direction (single sentence):** Evolve the Nexus design system into **“The Literary Engine”** — editorial serif voice for content, ink-blue atmospheric depth for dark surfaces, layered elevation/motion for premium tactility, and a unified chromatic canvas — contracted in DESIGN.md v0.4 and adopted end-to-end through tokens → component library → app surfaces → canvas → design-studio verification.

**Scale budget:** `L` → **4 business plans**. Harness process (Review chain, SDD, QC/QA, compound, close, PR, merge-ready) is not counted and not planned as business plans.

## Product story — why “The Literary Engine”

Nexus is a **local-first creative-writing tool**: authors orchestrate ideas on infinite canvases, read manuscripts, and command AI agents — not another generic dark SaaS dashboard. Today the UI is *functionally complete* but *expressively flat*: 100% system sans, neutral dark flip, three flat shadows, and Tailwind-palette leftovers on the signature canvas. That gap undermines trust and premium feel for a product whose themes are **创作 / 文学 / AI / 互联 / 画布 / 计算引擎**.

**The Literary Engine** reconciles two registers authors already live in:

| Register | Keywords | Visual language | Where it appears |
|----------|----------|-----------------|------------------|
| **Content voice** | 创作 · 文学 | Editorial display serif, calm reading measure, warm-paper light cast | Work/world/chapter titles, manuscript reading, empty-state headlines on authoring surfaces |
| **Interface voice** | AI · 互联 · 画布 · 计算引擎 | Precise system sans, ink-blue atmospheric darks, cyan signals, layered elevation | Shell, controls, canvas instrument chrome, status language |

**Author-visible outcomes (end of V1.121):**

1. **First launch feels like an atelier** — setup wizard and shell carry brand atmosphere, not installer chrome.
2. **Creative entities read as literature** — titles and novel chapter headings use the display serif; nav/buttons stay engine-precise.
3. **Canvas feels like the product’s instrument** — ambient ink surface, tactile nodes, brand-scale chromatics (no off-palette drift).
4. **Dark mode feels branded** — ink chamber, not a neutral `#0a0a0a` flip.
5. **Design system is verifiable** — design-studio galleries inspect every v0.4 category in light + dark (studio-first invariant, V1.106).

This is **elevation of existing surfaces**, not new features: same routes, same APIs, higher design quality and correctness (AA + parity).

## Scope

本迭代锁定的 spec 点（covers **design-studio + apps/web + `@42ch/nexus-ui`**; desktop inherits via web SPA wrap — no native Tauri chrome work):

- **S1 — Design language foundation (v0.4 contract + pipeline):** display/serif typography tier; ink-atmosphere dark surfaces + refined light tints; layered elevation scale; motion recipes; canvas ambient token set; canvas chromatic hygiene mapping; reading-chrome **token contract** (values authored here; CSS migration in S4); `tooling/design-tokens` projection (tokens.css + tailwind preset + tailwind-merge registration); self-hosted OFL serif decision + build wiring; studio **token** galleries (Typography / Spacing / Radius / Elevation / Motion).
- **S2 — Component library elevation:** `@42ch/nexus-ui` + keep-web `ui/` components adopt v0.4 (hover depth, focus, motion, disabled, scrim token); hardcoded-value sweep in the ui layer; studio components gallery states matrix.
- **S3 — App surfaces elevation:** shell chrome (sidebar/header/banner/status bar/footer/command palette/work rail), setup wizard, Control Room pages, settings, empty/error states; content-voice typography adoption; arbitrary-value sweep in pages; register `canvas-node-width-*` tokens for S4 (registration only — no canvas visual change in S3).
- **S4 — Canvas & reading elevation:** three canvas surfaces (Strategy/Outline/WorldKB) ambient + node chrome v2 + edges + per-surface accents; reading chrome CSS tokenized; display serif on reading surfaces; studio **canvas** galleries + iteration-wide light/dark parity + AA close-out sweep.

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-07-17-v1.121-design-language-foundation | P0 — Design language foundation (DESIGN.md v0.4 + token pipeline + studio token galleries) | Todo | **Must** — without v0.4 contract + pipeline, P1–P3 have nothing durable to adopt |
| 2026-07-17-v1.121-component-library-elevation | P1 — Component library elevation (nexus-ui + keep-web ui + studio gallery) | Todo | **Must** — surfaces inherit tactility from components; elevating pages without library = one-off drift |
| 2026-07-17-v1.121-app-surfaces-elevation | P2 — App surfaces elevation (shell + setup + Control Room pages) | Todo | **Must** — daily author path (shell/setup/Control Room); first impression + voice split live here |
| 2026-07-17-v1.121-canvas-reading-elevation | P3 — Canvas & reading elevation + studio canvas galleries + parity sweep | Todo | **Must** — signature surfaces (画布 + 阅读); iteration incomplete if only shell/components elevate; depends P0 + P1 + P2 (`canvas-node-width-*`) |

**Must integrity (no Stretch plans this iteration):** Caller asked for full-system elevation across studio + web + component library with correctness verification. Dropping any plan leaves an orphan register (language / controls / chrome / canvas). Residual polish after ship → V1.122 roadmap, not silent Stretch demotion of a Must plan.

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit chain complete, compass locked) | 2026-07-17 | pending |
| P0 foundation merged to integration | 2026-07-18 | pending |
| P1 component library merged | 2026-07-19 | pending |
| P2 app surfaces merged | 2026-07-20 | pending |
| P3 canvas & reading merged; all plans Done | 2026-07-21 | pending |
| Iteration close + PR merge-ready | 2026-07-21 | pending |

## Acceptance Criteria

Each AC is binary and evidence-backed (grep, contrast table, vitest/build log, and/or light+dark screenshot pack). “Looks better” is not acceptance.

- **AC-V1121-1** *(P0)* — DESIGN.md + DESIGN.dark.md ship v0.4.0: display typography tier, ink atmosphere, elevation scale, motion recipes, canvas ambient set, reading-chrome token **names/values**, chromatic mapping appendix; values projected to `tooling/design-tokens` (tokens.css + preset); `tailwind-merge` registrations updated; WCAG 2.1 AA contrast table recomputed for every changed pairing, light + dark.
- **AC-V1121-2** *(P1)* — `@42ch/nexus-ui` promoted set (Button, Badge, Card, Input, Label, Textarea, Select, Toast) and keep-web `ui/` (Dialog, Sheet, Tabs, Table, States) consume v0.4 tokens; zero unexplained hardcoded overlay/hex/color-mix arbitrary values in the ui layer + listed badge files (documented exceptions only); studio components gallery covers variants × states in both themes.
- **AC-V1121-3** *(P2)* — Shell chrome, setup wizard, and all Control Room pages adopt v0.4 (content-voice vs interface-voice rules, elevation, motion); arbitrary-value heatmap reduced to documented exceptions; **no intentional layout/IA/route changes** — existing page/wizard tests green; light+dark screenshot evidence for shell + wizard + ≥1 dense page + ≥1 empty state.
- **AC-V1121-4** *(P3)* — All three canvas surfaces render with ambient tokens (surface/grid/node/edge/minimap/controls) and brand-scale chromatics (zero Tailwind-palette leftover hexes in canvas tokens **and** canvas component source); reading chrome CSS is token-only; novel-profile chapter titles render the display serif in light + dark (screenshot + computed-style evidence).
- **AC-V1121-5** *(P0 + P3)* — Design-studio: P0 ships Typography, Spacing/Radius, Elevation/Motion galleries; P3 ships Canvas token + canvas surface galleries; every v0.4 token category is inspectable in-studio in both themes.
- **AC-V1121-6** *(all plans)* — `pnpm` builds + vitest suites green for `apps/web`, `apps/design-studio`, `packages/nexus-ui`, `tooling/design-tokens`; Rust/daemon untouched (`wire_contracts_changed: false` for all plans).
- **AC-V1121-7** *(P3 close-out — “确保没问题”)* — Iteration-wide light/dark **parity pack** (Strategy + Outline + WorldKB + Reading × both themes) + AA spot-check table for remapped canvas chromatics and ink-surface text pairings recorded on the P3 plan; no open BLOCKING residuals on contrast or chromatic hygiene at plan Done.

## Non-Goals

- **No wire contract / daemon / Rust changes** — pure frontend design iteration (`wire_contracts_changed: false`).
- **No new product features, routes, or IA** — elevation of existing surfaces only; settings IA (V1.103/V1.106) stays frozen.
- **No rebrand** — VI palette (`brand-deep-blue`, `brand-cyan`, `brand-white`) and logo assets are frozen; we add derived atmosphere (ink cast, elevation, motion), not new brand hues.
- **No component API redesign** — existing props/call sites stay compile-compatible. **Additive** opt-ins only (e.g. `Card.Title` content-voice flag) are allowed; no prop renames/removals. Package promotion (keep-web → `@42ch/nexus-ui`) is out of scope.
- **No i18n / copy rewrites** — locale keys and Verb-only voice (V1.117) unchanged; typography/layout only. Empty/error state **headline shape** may use display serif; string content is not rewritten for marketing.
- **No webfont for UI sans** — Inter/system stack stays; only the display/serif tier may add an optional self-hosted OFL font.
- **No desktop native chrome** — `apps/desktop` inherits web SPA elevation; no Tauri shell/titlebar/menu visual work.
- **No new motion/animation libraries** — token durations + CSS/Tailwind transitions only; no Framer Motion / spring system introduction.
- **No a11y IA restructure** — WCAG 2.1 AA contrast + existing focus/ARIA contracts preserved/re-verified; no full a11y audit or landmark redesign.
- **No canvas behavior / React Flow upgrade / performance rewrite** — visual + token only (P3 non-goals).
- **No manuscript editing features** — reading stays read-only (V1.79 boundary).
- **No Figma / design-tool export pipeline** — DESIGN.md + tokens + studio are the SSOT.
- **No nexus-platform (private repo) changes.**

## Roadmap Position

- **Current iteration（V1.121）**：Design language v0.4 “Literary Engine” — foundation → component library → app surfaces → canvas/reading, verified in design-studio. Delivers the premium, literary-computational identity the product surfaces currently lack — without shipping new features.
- **Next iteration**：V1.122 — post-elevation polish + any V1.121 residuals (including deferred self-hosted serif if P0 chose system-stack fallback); candidates: keep-web → package promotion decisions informed by v0.4 usage, canvas performance follow-ups, residual arbitrary-value exceptions. 触发条件：V1.121 shipped + dogfood feedback, owner：product-manager。
- **最终目标**：Every Nexus surface expresses one coherent literary-computational design language, authored in DESIGN.md, projected through tokens, verified in design-studio — no surface-local visual invention (V1.106 studio-first invariant).

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.121` |
| `target_branch` | `main` |

Branch resolve evidence (autonomous): `status.json` root metadata (`iteration_base_branch: main`, `target_branch: main`) + V1.118–V1.120 shipped compasses all `main → iteration/vX → main`.

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Dark ink-tint shift breaks AA on existing pairings | Med | High | P0 recomputes the full contrast table before lock; QA dark-theme sweep in P2/P3; studio parity gallery |
| Self-hosted serif adds bundle weight / font-loading jank | Med | Med | Subset + `font-display: swap` + system-serif fallback; measure delta in P0; opt-out keeps Georgia-grade stack |
| tailwind-merge strips new token classes (V1.94 regression class) | Med | High | P0 registers every new font-size/font-family/shadow/duration/min-width class group in `packages/nexus-ui/src/lib/cn.ts` (V1.100 SSOT, not "V1.94 location"; guarded by `tooling/check-ui-guardrails.sh`); P1 adds a regression test |
| Self-hosted font binary import violates package boundary | Med | Med | `packages/nexus-ui/AGENTS.md` forbids binary asset imports from component source; font files canonical in `packages/nexus-ui/assets/fonts/` (LFS), vendored to each app's `public/fonts/` with provenance comment; `@font-face` + `--font-display` in `tooling/design-tokens/src/tokens.css` (P0 spec T1) |
| Canvas chromatic remap alters status semantics authors rely on | Low | Med | Mapping table keeps hue families identical (blue→blue, green→green); only chroma family is unified |
| 4-plan visual churn confuses incremental review | Low | Low | Serial per-plan merges to integration; studio gallery evidence per plan; QA gate per plan |
| Arbitrary-value sweep touches many files → merge conflicts with dogfood fixes | Low | Low | Sweep is mechanical and plan-scoped; rebase on integration before each plan merge |
| P0/P3 ownership drift on reading-chrome (tokens vs CSS) | Med | Med | Spec T6 ownership split + plan Interfaces: P0 authors tokens, P3 migrates CSS only |
| Implementers invent surface-local colors “for premium feel” | Med | High | Grep gates + studio galleries; no new brand hues; exceptions must be plan-documented |

## Iteration package

> Sibling paths under `{ITERATION_DIR}/v1.121/` — not in `{SPECS_DIR}/` or `{KNOWLEDGE_DIR}/`. Promoted to knowledge at iteration-close via **`mstar-compound`**.

| Path | Purpose |
|------|---------|
| `guides/` | Exploration, process notes |
| `specs/` | Iteration-scoped spec drafts (P0–P3 primary specs) |
| `README.md` | Package document index |

## Quality Gate Summary

> Filled at iteration-close. Human summary only; per-plan gate details stay in each main plan, and open residual SSOT stays in `{HARNESS_DIR}/status.json`.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| (pending) | | | | |

Notes:

- Raw review bundle: `{SDD_DIR}/review/` (ephemeral; do not rely on it after Done).
- Open residual SSOT: `{HARNESS_DIR}/status.json` root `residual_findings[<plan-id>]`.

## Compound Round Summary

> Filled at iteration-close.

- 结晶文档数：(pending)
- 新增 CONCEPTS.md 条目：(pending)
- 触发 compound-refresh：(pending)

## Iteration Retrospective (minimal)

> Filled at iteration-close.

- 做得好的：
- 可改进的：
- 下迭代建议：
