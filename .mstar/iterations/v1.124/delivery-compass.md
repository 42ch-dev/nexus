---
iteration_id: V1.124
start_date: 2026-07-19
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.124
plans:
  - 2026-07-19-v1.124-p0-studio-timeline-fixtures
  - 2026-07-19-v1.124-p1-studio-tokens-gallery-completion
  - 2026-07-19-v1.124-p2-unrepresented-surface-audit
  - 2026-07-19-v1.124-p3-promotion-classification-audit
---

# V1.124 Delivery Compass — Studio-first Gap Closure: Timeline · Layer · Tokens

> **Direction lock mode: autonomous** (`/iteration-loop`, scale **L** — 4 business plans locked per caller direction).
> Locked direction, rationale, candidate trade-offs recorded below per `mstar-iteration` §1.2 autonomous + `references/autonomous-direction-lock.md`.
>
> **Phase 1 Review & Edit chain:** product-manager seat 1 → architect seat 2 → writing-specialist seat 3 → PM lock. Direction is **locked** — do not re-question the Studio-first gap closure for V1.122/V1.123 Timeline visuals.

## Autonomous direction lock record

**Caller constraint (direction arg):** 就上述上下文开始迭代，除增加 fixture 外还可以探索是否有更多未录入 design studio 但又适合收录的组件，也落实推进。Caller scale token: **M~L** → resolved as **L** (4 business plans) because the exploratory scope spans Timeline + Layer + Tokens + broader surface audit (distinct deliverables).

**Scale budget application (L = 3–4 business plans; harness process not counted):** 4 business plans locked — P0 Timeline fixtures (Must), P1 Tokens gallery completion (Must), P2 unrepresented surface audit (Should), P3 promotion classification audit (Stretch). Foundation artifact: already-drafted root `AGENTS.md` UI Component Policy section lands with P0 as the durable rule this iteration operationalizes.

**Branch policy (autonomous resolve per `references/autonomous-direction-lock.md`):**
- `iteration_base_branch: main` — resolved from `status.json` root `metadata.iteration_base_branch` (matches V1.122/V1.123 pattern).
- `target_branch: main` — resolved from `status.json` root `metadata.target_branch` (matches V1.122/V1.123 PR targets #156 / #157).
- `spec_integration_branch: iteration/v1.124` — new branch cut from `main`.

This is the **documented project policy** (V1.122 → PR #156, V1.123 → PR #157, both `iteration/vX → main`); not a silent `main` default.

### Candidates evaluated

Research base: V1.123 ship artifacts (`STRATEGY.md` Three Pillars — Timeline is the hero World entry surface; CONCEPTS.md Brief/Narrative/Moment layers), V1.122+V1.123 implementation inventory in `apps/web/src/components/canvas/` (timeline-canvas/ + work-timeline-canvas/ + world-kb/ + layer-breadcrumb.tsx + global-timeline/), `tooling/design-tokens/src/tokens.css` (V1.123 P3 T2 + P4 T2 added canvas-timeline-accent + canvas-layer-{brief,narrative,moment}-accent + soul-viz-timeline-axis-* tokens), `apps/design-studio/src/fixtures/canvas-surfaces-fixtures.tsx` (only mirrors Outline/Strategy/WorldKB chrome — Timeline absent), `apps/design-studio/src/pages/tokens.tsx` (1165 lines; lists canvas-strategy/outline/worldkb-accent but omits canvas-timeline-accent + canvas-layer-* family + soul-viz-timeline-axis-*), `packages/nexus-ui/src/index.ts` (no Timeline/Layer/canvas primitives — correctly, these are App-coupled RF nodes), root `AGENTS.md` (pre-iteration edit adds Studio-first UI Component Policy motivated by this exact gap), V1.106 Studio-first invariant, `.mstar/knowledge/architecture-patterns/ui-component-promotion-workflow.md`.

| # | Candidate | Trade-off | Verdict |
|---|-----------|-----------|---------|
| A | **Studio-first Gap Closure (Timeline + Layer + Tokens + audit)** — close the V1.122/V1.123 gap holistically: Timeline fixtures (P0) + Tokens gallery (P1) + audit of other unrepresented surfaces (P2) + promotion classification (P3); operationalizes the just-landed root `AGENTS.md` UI Component Policy | Largest surface breadth (touches Studio fixtures, Tokens page, audit); but matches caller direction exactly (close Timeline gap + explore other unrepresented components + 落实推进) and binds the policy rule to evidence in the same iteration that motivated it | **LOCKED** — directly implements every clause of caller direction; the Q&A that triggered this iteration named Timeline as the example, but caller explicitly said "explore whether there are more components not in design-studio that are suitable" |
| B | Timeline-only fixtures (World + Work Timeline + Global Timeline; no Tokens gallery, no broader audit) | Smaller scope; but leaves Tokens gallery defect open (tokens in CSS but invisible in Studio — a rule the new AGENTS.md policy explicitly calls out as a defect requiring a residual) and ignores caller's "explore more components" clause | Rejected (partial — leaves Tokens gallery defect open + ignores caller's exploratory clause) |
| C | Studio-only work (fixtures + Tokens gallery); no `@42ch/nexus-ui` promotion audit | Fastest; but caller said "也落实推进" (actually push forward), and the AGENTS.md policy explicitly binds fixture work to a promotion decision — skipping P3 leaves the Studio-first promotion loop half-built | Rejected (insufficient — no promotion decisions recorded) |
| D | `@42ch/nexus-ui` Timeline primitive promotion (move Timeline node components into the package) | Aligns with "reusable presentational primitive" rule; but Timeline node types are RF-coupled (`@xyflow/react` types in `timeline-node-types.tsx`) and the V1.106 Studio-first pattern explicitly keeps RF-coupled chrome app-local via `@web-canvas/*` extracts. Premature promotion violates V1.106 invariant. | Rejected (premature — RF-coupled; promotion audit P3 will document why these stay app-local) |
| E | Defer to V1.125; status.json has 77 open residuals, focus this iteration on residual cleanup | Real tech-debt pressure; but caller direction explicitly redirects to Studio-first gap closure this iteration. Residual cleanup is roadmap-tracked (see Roadmap Position). | Rejected (wrong direction — caller redirected; residual cleanup deferred to roadmap) |

### Evidence base for A

- **V1.122 + V1.123 shipped Timeline visually to `apps/web` only.** `apps/web/src/components/canvas/timeline-canvas/timeline-node-types.tsx` defines three node kinds (`timeline-brief-era`, `timeline-event`, `timeline-key-block`); `work-timeline-canvas/work-timeline-node-types.tsx` defines Work Timeline Narrative + Moment nodes; `apps/web/src/components/global-timeline/global-timeline-view.tsx` is the cross-World overview (V1.123 P3 T1). None have Studio fixtures.
- **Studio Canvas Surfaces fixture is incomplete.** `apps/design-studio/src/fixtures/canvas-surfaces-fixtures.tsx` mirrors only Outline + Strategy + WorldKB chrome (3 of 6 `CanvasSurfaceKind` values). The Timeline, Work Timeline, and World KB alt-view fixtures are absent.
- **Tokens are in CSS but invisible in Studio.** `tooling/design-tokens/src/tokens.css` (856 lines, 464 token declarations) contains `--color-canvas-timeline-accent`, `--color-canvas-layer-brief-accent`, `--color-canvas-layer-narrative-accent`, `--color-canvas-layer-moment-accent` (V1.123 P3 T2 + P4 T2), `--color-canvas-outline-timeline-event-pin`, `--color-canvas-outline-timeline-marker`, `--color-soul-viz-timeline-axis-{line,tick,label}`. The Studio Tokens page (`apps/design-studio/src/pages/tokens.tsx`, 1165 lines) registers `canvas-strategy-accent` / `canvas-outline-accent` / `canvas-worldkb-accent` but **not** the Timeline/Layer/Soul-viz families. This is exactly the defect class the new `AGENTS.md` UI Component Policy § "Tokens need a gallery" codifies.
- **Layer breadcrumb is documented as keep-app-local + Studio-mirror.** `apps/web/src/components/canvas/layer-breadcrumb.tsx` header comment: "Built as a small inline-style component (not promoted to `@42ch/nexus-ui`) because the layer-chain shape is canvas-specific... if a third surface arrives with the same pattern, promotion becomes worth the abstraction cost." → Studio mirror via `@web-canvas/layer-breadcrumb` is the correct path (parallel to V1.115 `@web-canvas/node-chrome-shell`).
- **`@42ch/nexus-ui` has no Timeline/Layer primitives (correctly).** `packages/nexus-ui/src/index.ts` exports Button/Badge/Card/Input/Label/Textarea/Select/Toast + brand. Timeline nodes are RF-coupled → promotion-blocked per V1.106 invariant; P3 records this decision rather than forcing promotion.
- **Root `AGENTS.md` UI Component Policy edit is already on disk (uncommitted).** Adds "Studio-first" decision rule, workflow, anti-patterns (Timeline explicitly named), and authority links. Lands with P0 as the durable rule this iteration operationalizes.
- **Studio-first workflow is already proven.** `.mstar/knowledge/architecture-patterns/ui-component-promotion-workflow.md` + V1.99 + V1.106 + V1.108 + V1.111 + V1.121 all validated the pattern (fixture → token gallery → optional package promotion → App integration). This iteration applies the same recipe to the V1.122/V1.123 Timeline surface gap.
- **Existing Studio import aliases cover the needed extract path.** `@web-canvas/*` (V1.115 — node-chrome-shell) precedent exists; adding `@web-canvas/timeline-node-types` or mirroring NodeChromeShell composition patterns follows the established boundary.

### Locked direction (single sentence)

Close the V1.122/V1.123 Studio-first gap so a contributor can review Timeline visuals and tokens **without the daemon**: (P0) land the Studio-first UI Component Policy in root `AGENTS.md` **first**, then World + Work Timeline node-chrome fixtures (Brief/Narrative/Moment via `@web-canvas/*` extracts) meeting `studio-fixture-acceptance-criteria.md`; (P1) complete the Studio Tokens gallery for missing canvas-timeline / canvas-layer / canvas-outline-timeline / soul-viz-timeline-axis tokens + a recurrence residual gate; (P2) audit and fixture unrepresented surfaces in product order (Global Timeline → Layer breadcrumb → conflict-modal family → alt-view toggles); (P3) record promotion classifications for reuse (not tidiness), keeping RF-coupled pieces app-local per V1.106.

### Dependency graph (locked)

```
P0 (policy + Timeline fixtures)  ← Must, no upstream
   ├── P1 (Tokens gallery)       ← Must, independent (different file)
   ├── P2 (surface audit)        ← Should, independent (different fixtures)
   └── P3 (promotion audit)      ← Stretch, benefits from P0/P1/P2 evidence but can Prepare in parallel
```

P0, P1, P2 touch disjoint files (Timeline fixtures vs Tokens page vs other-surface fixtures); they may Prepare in parallel but Execute **serially** (P0 → P1 → P2 → P3) per `mstar-iteration` §2.6 per-plan loop. P3 records promotion decisions based on P0/P1/P2 evidence; benefits from running last.

## Product story — who benefits and how

### One-sentence thesis

> **If Timeline is the hero instrument, its chrome and tokens must be reviewable in Design Studio without starting the product — otherwise every future visual change re-pays the V1.122/V1.123 "App-only gallery" tax.**

### Users of this iteration (not authors)

| Audience | Job after V1.124 |
|----------|------------------|
| **Contributor / frontend implementer** | Open Studio → accept Timeline node chrome + tokens offline; land new visuals under the written Studio-first policy |
| **QC / visual reviewer** | Fail PRs that ship App chrome or CSS tokens without Studio evidence (F1–F9 + Tokens gallery) |
| **Authors (local Web UI)** | **No change** this iteration (NG-10) — they never open design-studio; App behavior stays V1.123 baseline |

### Why "gap closure" not "new feature"

V1.123 shipped three-layer Timeline **feel** in `apps/web`. The product bet already landed. V1.124 is the **verification surface** that makes that bet maintainable: gallery-first acceptance, token completeness, sibling-surface audit, and written placement decisions so the next Timeline change starts in Studio by default.

## Scope

本迭代锁定的 spec 点（**Studio-first gap closure** = V1.122/V1.123 shipped Timeline visuals into `apps/web` + tokens into CSS, but a contributor still cannot review those visuals or tokens in `apps/design-studio` without running the daemon — this iteration closes that review gap and binds the new root `AGENTS.md` UI Component Policy to evidence）：

- **S1 (P0 — Must)**: World Timeline node chrome (Brief-era / Event / KeyBlock Context cluster) and Work Timeline node chrome (Narrative / Moment) become reviewable in Studio — light + dark, all variants, **no daemon**. Visual acceptance for V1.122/V1.123 Timeline **node chrome** moves out of `apps/web`-only smoke. (Global Timeline overview is **not** S1 — it is S3/P2.)
- **S2 (P1 — Must)**: A contributor opening Studio's Tokens page can see every canvas Timeline / Layer / Soul-viz-timeline token family that already exists in `tokens.css` (light + dark values). "Token in CSS but invisible in Studio" is a defect this iteration eliminates for the V1.122/V1.123 backlog, and codifies as a recurrence gate.
- **S3 (P2 — Should)**: Surfaces that authors already touch in shipped App chrome but Studio never mirrors — **priority order**: (1) Global Timeline overview, (2) Layer breadcrumb, (3) conflict-modal family, (4) alt-view toggles — are audited against the V1.106 four-bucket rubric; Studio-eligible pieces get fixtures; non-eligible pieces get written keep-app-local rationale (not silent omission).
- **S4 (P3 — Stretch)**: Every Timeline/Layer/canvas piece touched by S1–S3 carries a recorded promotion classification (`promoted primitive` / `studio-local fixture` / `web-only wrapper` / `future web product component`). Classification serves **cross-app reuse** (does a second consumer need this pure presentational shell?), not architectural tidiness. RF-coupled pieces stay app-local with cited V1.106 rule.

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-19-v1.124-p0-studio-timeline-fixtures` | P0 — Studio-first Timeline fixtures + AGENTS.md policy land | Todo | Must; foundation; no upstream |
| `2026-07-19-v1.124-p1-studio-tokens-gallery-completion` | P1 — Studio Tokens gallery completion | Todo | Must; disjoint files from P0 |
| `2026-07-19-v1.124-p2-unrepresented-surface-audit` | P2 — Unrepresented surface audit + fixtures | Todo | Should; disjoint from P0/P1 |
| `2026-07-19-v1.124-p3-promotion-classification-audit` | P3 — Promotion classification audit + decisions | Done | Stretch; benefits from P0/P1/P2 evidence |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Phase 1 compass locked | 2026-07-19 | in-progress |
| P0 fixtures accepted in Studio | 2026-07-19 | pending |
| P1 Tokens gallery complete | 2026-07-19 | pending |
| P2 audit complete + fixtures landed | 2026-07-20 | pending |
| P3 promotion decisions recorded | 2026-07-19 | in-progress |
| Iteration close + PR | 2026-07-20 | pending |

## Acceptance Criteria

Observable product criteria (each AC maps to exactly one plan or process gate; no orphans):

- **AC-V1124-1** (P0 → S1): A contributor runs `pnpm --filter design-studio dev` (no daemon, no Tauri) and on the Canvas Surfaces page can **see and compare** World Timeline node chrome (Brief-era + Event + KeyBlock Context cluster) and Work Timeline node chrome (Narrative event + Moment **scene and beat**) in **light and dark**, with all status/drag/badge variants visible, without console errors. Visual acceptance for V1.122/V1.123 Timeline **node chrome** happens here — not by launching `apps/web`. **Technical observability:** fixtures import `@web-canvas/node-chrome-shell` + `@web-canvas/timeline-node-chrome` (same modules App RF wrappers use); smoke tests + F1–F9 per `specs/studio-fixture-acceptance-criteria.md` §8; boundary matrix in `specs/studio-timeline-fixture-boundaries.md`.
- **AC-V1124-2** (P0 → S1 foundation): Root `AGENTS.md` ships the UI Component Policy (Studio-first) section, including the Timeline anti-pattern reference and Authority links. P0–P3 plans cite it as the durable rule; a new contributor reading only root `AGENTS.md` can answer "where do I land a new visual surface?"
- **AC-V1124-3** (P1 → S2): A contributor opens Studio Tokens page, toggles light/dark, and finds every canvas Timeline / Layer / Soul-viz-timeline token that exists in `tooling/design-tokens/src/tokens.css` as a labeled swatch (including at minimum: `canvas-timeline-accent`, `canvas-layer-{brief,narrative,moment}-accent`, `canvas-outline-timeline-{event-pin,marker}`, `soul-viz-timeline-axis-{line,tick,label}`). No V1.122/V1.123 canvas Timeline/Layer/Soul-viz token remains CSS-only. **Technical observability:** nine-row delta table + locked group titles in `specs/tokens-gallery-audit.md`; DOM smoke asserts all nine labels; recurrence gate text present.
- **AC-V1124-4** (P2 → S3): Audit doc lists every file under `apps/web/src/components/canvas/**` + `global-timeline/**` with a four-bucket classification. For each **Studio-eligible** piece in the locked priority set (Global Timeline → Layer breadcrumb → conflict-modal family → alt-view toggles), a Studio fixture exists and meets `studio-fixture-acceptance-criteria.md`. Every non-eligible piece has a written keep-app-local rationale (no silent gaps). **Technical observability:** extract paths locked in `specs/surface-audit-checklist.md` (`@web-global-timeline/*`, `@web-canvas/layer-breadcrumb`, `@web-canvas/conflict-modal-chrome`); new alias lands only for Global Timeline; `./tooling/check-ui-guardrails.sh` green.
- **AC-V1124-5** (P3 → S4): Every piece fixture-ed or audited in P0/P1/P2 has a row in P3's promotion classification table. Any new `@42ch/nexus-ui` promotion is recorded in `packages/nexus-ui/AGENTS.md` with ≥2 consumers + presentational props contract. Keep-app-local / studio-local decisions cite the specific V1.106 rule (RF-coupled / daemon-coupled / single-consumer / etc.) — promotion is never "because it looks tidy."
- **AC-V1124-6** (process): No new `{KNOWLEDGE_DIR}/` documents from Phase 1 Review chain. Knowledge crystallization deferred to Phase 3 `mstar-compound`.
- **AC-V1124-7** (process): Compass `status: locked` after PM lock; all four plans registered in `status.json` with `spec_integration_branch: iteration/v1.124`.

**AC → plan map (no orphans):** AC-1/2 → P0 · AC-3 → P1 · AC-4 → P2 · AC-5 → P3 · AC-6/7 → process.

## Non-Goals

Concrete exclusions (if a PR does any of these, it is out of V1.124 scope):

- **NG-1**: No `@42ch/nexus-ui` promotion of RF-coupled Timeline node components (premature — V1.106 invariant). P3 **records** keep-app-local; it does not force package moves "for cleanliness."
- **NG-2**: No `apps/web` Timeline visual rework, density retune, or three-layer zoom UX change. V1.122/V1.123 shipped visuals are the baseline; this iteration **mirrors** them in Studio. Aesthetic tuning is a **later** iteration, evidence-backed by the new fixtures — not piggybacked here.
- **NG-3**: No daemon routes, wire contracts, schemas, or `crates/nexus-*` changes. No `apps/web/src/lib/nexus/**` consumer changes. Touch surfaces are `apps/design-studio`, optional presentational extracts under `apps/web/src/components/**/presentational/`, optional `@web-*` alias/guardrail updates, `packages/nexus-ui` docs only if a genuine promotion lands, and root `AGENTS.md`.
- **NG-4**: No `status.json` compaction and no open-residual cleanup sweep (`tech_debt_summary.total_open: 77` stays roadmap). Do not burn L budget on harness hygiene this iteration.
- **NG-5**: No Layer breadcrumb (or Global Timeline list) promotion to `@42ch/nexus-ui` unless P3 finds ≥2 pure-presentational consumers **and** a stable props contract. Default is Studio-mirror / keep-app-local (header comment already says "until a third surface").
- **NG-6**: No rewrite of existing Studio Outline/Strategy/WorldKB fixtures (`canvas-surfaces-fixtures.tsx` stays; P0 **adds** Timeline sections alongside).
- **NG-7**: No new i18n keys / `t()` for Studio fixtures (developer-facing surface; static English labels only). Fixture copy still follows DESIGN.md **Voice & Content** tone (clear, non-marketing labels) — English-only does not mean free-form slang.
- **NG-8**: No product-pillar work deferred from V1.122/V1.123 roadmap — Fork UI, Computable UI, Harness rename, compute-on-timeline, World-Moment / Work-Brief layer promotions. Those stay roadmap candidates (see Roadmap Position).
- **NG-9**: No new design tokens in P0/P1 "because the gallery looks empty." P1 only projects tokens that already exist in `tokens.css`. Missing-on-both-sides tokens → residual / roadmap, not silent invention in the gallery.
- **NG-10**: No author-facing product behavior change. Authors never open design-studio; this iteration does not alter Control Room / Canvas runtime behavior for authors.

## Roadmap Position

- **Current iteration (V1.124)**: Studio-first gap closure — make V1.122/V1.123 Timeline visuals and tokens **reviewable without the daemon**, audit sibling unrepresented surfaces, and close the promotion-decision loop.
- **Next iteration (V1.125 candidate) — product priority order** (caller may override at open; default PM autonomous resolve follows this order):
  1. **Fork creation/merge UI** (`DF-V1122-FORK-UI`) — **preferred next product bet.** Timeline is now the hero World instrument (V1.122/V1.123) and will be gallery-verifiable (V1.124); Fork is the missing author action that makes multi-timeline World history real. Highest PMF adjacency to the Timeline stack just shipped.
  2. **Computable pillar UI** (`DF-V1122-COMPUTABLE-UI`) — elevates the third STRATEGY pillar after Timeline/Canvas depth. Strong product value; slightly less adjacent to the just-closed Timeline visual stack than Fork.
  3. **Harness UI rename** (`DF-V1122-HARNESS-RENAME`) — polish / IA honesty; lower PMF urgency than Fork or Computable.
  4. **status.json compaction + residual cleanup** (77 open) — continuous harness hygiene; may piggyback as a non-business pre-P-last gate inside a product iteration, but should not **be** the iteration direction unless caller redirects to debt-paydown.
- **Promotion triggers recorded this iteration (P3):**
  - **Layer breadcrumb** (`@web-canvas/layer-breadcrumb`): promote to `@42ch/nexus-ui` when a **third** surface reuses the same layer-chain pattern (per source header comment). Currently two consumers: App (i18n adapter) + Studio (fixture); neither is a pure presentational consumer.
  - **Timeline node chrome** (`@web-canvas/timeline-node-chrome`, 6 exports): promote when a **non-canvas consumer** needs Timeline body chrome. Currently App RF wrappers + Studio fixtures — both canvas-context.
  - **Conflict-modal chrome** (`@web-canvas/conflict-modal-chrome`): promote when a **non-canvas consumer** needs conflict modal chrome. Currently App domain wrappers + Studio fixture — both canvas-context.
  - **Global Timeline list chrome** (`@web-global-timeline/global-timeline-list-chrome`): promote when a **non-daemon, non-canvas consumer** needs list chrome rows. Currently daemon-coupled App view + Studio fixture.
  - **No `future web product component`** candidates identified — all deferred surfaces are `web-only wrapper` with revisit triggers, not new product UI.
- **最终目标**: Every Nexus surface expresses one coherent literary-computational design language — authored in DESIGN.md, projected through tokens, **verified in design-studio** — no surface-local visual invention (V1.106 Studio-first invariant). V1.121 locked the contract; V1.122/V1.123 applied it to Timeline in App; V1.124 makes that work **gallery-verifiable** so future visual change is cheap and policy-enforced.

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.124` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Timeline node chrome depends on RF types not extractable to `@web-canvas/*` | **Low** (after seat 2) | High | **Locked:** RF wrappers stay App-local; body chrome extracts to `@web-canvas/timeline-node-chrome` (existing alias root). See `specs/studio-timeline-fixture-boundaries.md`. |
| **Layer-accent migration during extract** — Event/Narrative badges today use `worldkb-accent`; extract moves them to `layer-narrative-accent` | Medium | Medium | Intentional V1.123 P4 completion inside shared extract (boundary §4.2/§4.4). Not a density redesign (NG-2). QC verifies both App + Studio pick up the extract. |
| Tokens gallery page becomes too long (1165 lines + new tokens) | Low | Low | **Locked IA:** four groups — Canvas — Timeline accent spine / Layer accents / Outline Timeline pins / Soul Viz — Timeline axes (`tokens-gallery-audit.md` §4). |
| Surface audit (P2) discovers too many candidates for one iteration | Medium | Medium | Classify first; fixture only the locked priority set; alt-views fixture-only-if-cheap; everything else → roadmap (`surface-audit-checklist.md`). |
| **New `@web-global-timeline` alias** needs vite/tsconfig/tailwind/AGENTS/guardrails updates | Medium | Medium | P2 Task 2 owns the full alias land; CI `check-ui-guardrails.sh` must pass before merge. P0 does **not** need a new alias root. |
| Phase 1 Review chain diverges on scope | Low | Medium | Direction locked autonomous; specialists edit within § Scope + Plans. Out-of-scope ideas → roadmap only. |
| **Studio-first policy onboarding fails** — new contributors still land visuals in `apps/web` first | Medium | High | P0 lands root `AGENTS.md` policy **before** fixture work (task order); P1 Task 3 writes recurrence gate; writing-specialist seat 3 judged a separate guide redundant (policy + F1–F9 + recurrence gate already cover the ground). Policy must be discoverable from root AGENTS alone. |
| **Fixture copy / Voice drift** — Studio labels use slang or invent product terms not in DESIGN Voice & Content | Medium | Medium | Global Constraints on P0/P2: fixture strings are static English but must use product vocabulary (Brief / Narrative / Moment / Timeline / Layer) and DESIGN Voice tone. No marketing fluff; no ACP jargon. Acceptance criteria doc names the check. |
| **Fixture ↔ App visual fidelity drift** — Studio chrome looks "close enough" but diverges from shipped App | Medium | High | **Locked F4 map:** same `@web-canvas/timeline-node-chrome` (P0), `@web-global-timeline/*` / `@web-canvas/layer-breadcrumb` / `@web-canvas/conflict-modal-chrome` (P2). Parallel badge JSX is reject (`studio-fixture-acceptance-criteria.md` §8). |
| **Priority thrash in P2** — idea-input / nav-commands crowd out Global Timeline | Low | Medium | Compass S3 locks product priority order; P2 Task 1 must not reorder without compass edit. |
| **Conflict-modal i18n in extract** — base uses `useTranslation` defaults | Low | Medium | Extract is props-first for all visible strings; App wrappers pass `t()`; Studio passes static English (`surface-audit-checklist.md` §4.3). |

## Iteration package

> Sibling paths under `.mstar/iterations/v1.124/` — not in `specs/` or `knowledge/`. Promoted to knowledge at iteration-close via `mstar-compound`.

| Path | Purpose |
|------|---------|
| `specs/studio-fixture-acceptance-criteria.md` | **Product contract** — F1–F9 + architect §8 testability / F4 extract map; reusable across P0/P2 |
| `specs/studio-timeline-fixture-boundaries.md` | **Architect locked** — Timeline body extract `@web-canvas/timeline-node-chrome` + per-kind accent map; P0 consumes |
| `specs/tokens-gallery-audit.md` | **Architect locked** — nine-token delta + gallery IA group titles + recurrence skeleton; P1 consumes |
| `specs/surface-audit-checklist.md` | **Architect locked** — P2 pre-classify + extract/alias decisions (incl. `@web-global-timeline`) |
| `guides/studio-first-policy-rollforward.md` | Process note: how root `AGENTS.md` UI Component Policy applies to V1.122/V1.123 backlog; recurrence template. _(Declined during writing-specialist seat 3 — policy + F1–F9 spec + P1 recurrence gate already cover the ground; redundant guide risk.)_
| `README.md` | Package document index |

## Quality Gate Summary

> Filled at iteration-close. Human summary only; per-plan gate details stay in each main plan, and open residual SSOT stays in `.mstar/status.json`.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| `2026-07-19-v1.124-p0-studio-timeline-fixtures` | TBD | TBD | TBD | TBD |
| `2026-07-19-v1.124-p1-studio-tokens-gallery-completion` | TBD | TBD | TBD | TBD |
| `2026-07-19-v1.124-p2-unrepresented-surface-audit` | TBD | TBD | TBD | TBD |
| `2026-07-19-v1.124-p3-promotion-classification-audit` | TBD | TBD | TBD | TBD |

Notes:

- Raw review bundle: `{SDD_DIR}/review/` (ephemeral; do not rely on it after Done).
- Open residual SSOT: `.mstar/status.json` root `residual_findings[<plan-id>]`.

## Compound Round Summary

> Filled at iteration-close.

- 结晶文档数：TBD
- 新增 CONCEPTS.md 条目：TBD
- 触发 compound-refresh：TBD

## Iteration Retrospective (minimal)

> Filled at iteration-close.

- 做得好的：TBD
- 可改进的：TBD
- 下迭代建议：TBD
