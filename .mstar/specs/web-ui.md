# Local Web UI (Control Room + Setup → Content-Authoring) — Specification v1

**Status**: Shipped (V1.65) — Control Room + Setup MVP (V1.64) **+ Content-Authoring UI stage (V1.65, §13)**: outline rich-text editor + chapter structure table + structure CRUD (slug/wc/volume/status; title display-only) + body read-only render + browser "Copy path" context menu. Tauri desktop shell + body full-text editor + "open-with" → **V1.66** (compass §0 Q5). QC tri-review Approve (fix-wave-1) + QA Pass. **+ V1.67 Surface Convergence & De-risk (§15)** + **V1.69 Design System Maturation & Canvas Draft** (`apps/web/DESIGN.md` Production + Canvas Draft) + **V1.70 Canvas Strategy Implement α (§16)** + **CI/desktop-build optimization** (parallel ops track; PR path filter narrowed + release-gated full build) + **V1.71 Canvas Strategy Write-Boundary (§17)** (Strategy patch routes, graphRevision conflict policy, conflict modal UX, canvas-write tokens) + **V1.72 Canvas Outline+Timeline β (§18)** (3 outline/timeline patch routes `outline.patch_structure` / `outline.patch_chapter` / `timeline.patch_event` + outlineRevision conflict policy + outline-flavored conflict modal UX + non-spatial alternate views + 8 outline/timeline canvas-write DESIGN.md tokens). V1.71 `wire_contracts_changed: TRUE` for Strategy; V1.72 `wire_contracts_changed: TRUE` for additive Outline+Timeline (`@42ch/nexus-contracts` 0.7.0 → 0.8.0); V1.73 `wire_contracts_changed: TRUE` for additive World KB (`@42ch/nexus-contracts` 0.8.0 → 0.9.0). **V1.74 Shipped** — Canvas World KB Relationships β (§20) with typed relationship edges, `world_kb.patch_relationship`, relationship inspector, non-spatial relationship table, and KB-flavored conflict modal reuse. **V1.94 Draft amendment** — §29 Information Architecture (V1.94): two-tab sidebar, nested nav, footer Profiles switcher, daemon status bar simplification, Strategies unification, button contrast invariant. **V1.98 Draft amendment** — §30 Design Studio dev surface (auxiliary gallery app; not author-facing). **V1.118 Draft amendment** — §29.17 Creation peer groups (Works / Worlds / Memories) + Canvas-first work shell (`WorkShellLayout` + `WorkRail`). **V1.125 Draft amendment** — §29.17.4 Worlds-first Creator list-mode sidebar (supersedes §29.17.1 peer groups only). **V1.122 Draft amendment** — §29.18 Three-pillar pivot (Harness/Canvas/Computable) + Timeline-first Canvas IA: Timeline is the default surface for **World entry** (`/worlds/:worldId` → Timeline); Work entry stays Outline (V1.118); `CanvasSurfaceKind = "timeline"` added as a peer surface. `wire_contracts_changed: false`.
**Document class**: Feature line  
**Created**: 2026-06-24  
**Scope**: Nexus local Web UI product contract — placement (`apps/web`), stack, daemon-served model, `tauri-api` adapter boundary, MVP surface (Control Room + Setup), Content-Authoring stage (V1.65), Tauri / body-editor roadmap (V1.66), and strict separation from the private cloud SaaS  
**Iteration compass**: [v1.64/delivery-compass.md](../iterations/v1.64/delivery-compass.md) (V1.64 ship) · [v1.65/delivery-compass.md](../iterations/v1.65/delivery-compass.md) (V1.65 Content-Authoring stage) · [v1.69/delivery-compass.md](../iterations/v1.69/delivery-compass.md) (V1.69 Design System Maturation & Canvas Draft — DESIGN.md Production migration + Canvas Exploration → Draft) · [v1.70/delivery-compass.md](../iterations/v1.70/delivery-compass.md) (V1.70 Canvas Strategy Implement α + CI/desktop-build optimization — the first canvas surface ships) · [v1.71/delivery-compass.md](../iterations/v1.71/delivery-compass.md) (V1.71 Canvas Strategy Write-Boundary β + hygiene companion)

**Coordinates with**:

- [cli-spec.md](cli-spec.md) §6.3 (daemon command group — Web UI access) + §7.1 (first-run path)
- [daemon-runtime.md](daemon-runtime.md) §2 (normative layering) — static-asset serving on the axum router
- [schemas-external-consumer-boundary.md](../knowledge/schemas-external-consumer-boundary.md) — the bundled UI is a first-class external consumer of `@42ch/nexus-contracts`
- [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) §1 — strict local-product vs cloud-product separation
- Repo-root [`DESIGN.md`](../../DESIGN.md) + [`DESIGN.dark.md`](../../DESIGN.dark.md) — sole normative DESIGN pair *(V1.98: supersedes former `apps/web/DESIGN*.md` — see §30)*
- [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md) (NEW, `@architect`-authored Master) — cursor pagination / `ErrorResponse` / naming conventions the UI data layer relies on

---

## 1. Purpose

Through V1.63 the local-first runtime is **feature-complete for writing but only reachable from the terminal**. Every operational action — see my Works, watch an orchestration session, inspect findings, configure a preset, start a Work — requires remembering `nexus42` commands.

V1.64 takes Nexus from **CLI-only** to **CLI + local Web UI**. This spec defines the product contract for a daemon-served, type-safe, local-first Web UI whose MVP is the **Control Room + Setup** surface:

- **Control Room** — read-heavy visibility into what the runtime is doing (Works, sessions, schedules, capabilities, findings).
- **Setup** — the write surface that configures the creative starting point (Work CRUD, preset management CRUD).

Content *production* (chapter rich-text, outline/KB editors) stays on the CLI this iteration and moves into the UI in V1.65+. The Web UI is the single highest-leverage product-completeness move because it makes the runtime legible and configurable to authors who are not terminal-fluent, without altering the daemon's data model or persistence.

---

## 2. Placement and product separation (normative)

### 2.1 Placement: OSS repo `apps/web`

The local Web UI lives in **this OSS repository** at `apps/web/` (a pnpm workspace member under `apps/*`), **not** in the private `nexus-platform` monorepo.

Rationale (frozen, compass §0 Q2):

1. **Build coupling.** The release build embeds the SPA bundle into the `nexus42` binary via `rust-embed`. The OSS binary build must not depend on a private repo's build graph; otherwise the public binary cannot be reproduced from the public repo.
2. **Type coupling.** The UI consumes `@42ch/nexus-contracts` via `workspace:*` so there is zero cross-repo version lag between wire schemas and the UI types. A private-repo placement would reintroduce npm-semver drift that V1.63's codegen promotion was meant to eliminate.
3. **Audience coupling.** This UI is a *local-first* surface for the local product line; it shares nothing with the cloud SaaS deployment model.

### 2.2 Strict separation from the private cloud SaaS

This is a **different product** from any web UI in the private `nexus-platform`:

| Dimension | Local Web UI (this spec, OSS) | Cloud SaaS (private `nexus-platform`) |
| --- | --- | --- |
| Deployment | bundled into the local `nexus42` binary; served from `localhost` | hosted multi-tenant cloud |
| Data source | local `state.db` + reference store via loopback Daemon API | platform HTTP / cloud DB |
| Audience | a single author on their own machine | platform tenants / cloud users |
| Auth | loopback only (keyless on `localhost`; see §4.2) | platform auth / sessions |
| Roadmap home | this spec + `apps/web/` | `nexus-platform` `v1-spec/` |

**Invariant:** no cloud-product feature, platform auth flow, or platform-gated capability (DF-13/16/55/59; PD-05) is exposed in this UI while `platform_integration = paused`. The UI surfaces only the local product line. Cross-repo contract sharing is one-way: this repo's `schemas/` → `nexus-contracts`; the UI never imports platform-only types.

---

## 3. Stack (normative)

| Layer | Choice | Why |
| --- | --- | --- |
| Framework | **React 18** | largest ecosystem; mental-model consistency with the existing `@42ch/nexus-contracts` TS consumer surface |
| Build / dev server | **Vite** (SPA) | matches "single-binary local-first"; no Node runtime required in the shipped product (build-time only) |
| Language | **TypeScript** (strict) | non-negotiable; the whole point of V1.63 codegen is end-to-end type safety |
| Styling | **TailwindCSS** | utility-first, low design-debt, pairs with the component layer |
| Component primitives | **shadcn/ui** | copy-in components keep ownership inside the repo; no opaque runtime dependency |
| Server state | **TanStack Query** | matches the cursor-pagination + shared `ErrorResponse` retry model; mature |
| Client routing | **React Router** | standard SPA routing for the screen groups |
| Wire types | **`@42ch/nexus-contracts`** via `workspace:*` | zero version lag with `schemas/`; the UI is a first-class external consumer |

This stack is the **Tauri-ready** foundation: it introduces no browser-only API in core logic, so the V1.65 desktop shell wraps the same `apps/web/dist` without a frontend rewrite (see §5, §9).

---

## 4. Serving and access model

### 4.1 Two serving modes

- **Release** — the built `apps/web/dist` is embedded into the `nexus42` binary via **`rust-embed`** and exposed by the daemon router through **`tower-http::ServeDir`-style** static serving semantics at the server root (`/`). The same binary that runs the runtime serves the UI. (Embedding strategy is finalized in plan P3; `serve-from-disk` under `~/.nexus42/web/` is the fallback only if embedding creates release-pipeline friction.)
- **Dev** — `apps/web` runs the **Vite dev server**, which proxies `/v1/daemon/*` to the running daemon (`nexus42 daemon start`). No embedding in dev; hot reload against the live Daemon API.

The static shell (HTML/JS/CSS assets) is **unauthenticated** by design: it carries no data. All data flows through the Daemon API.

### 4.2 Auth model (unchanged from the daemon)

The Web UI introduces **no new auth surface**. It inherits the daemon's existing loopback model (V1.20 compass): Daemon API data endpoints are reachable on `localhost` and are **keyless on loopback**; the static shell needs no credential because it holds no data. The UI does not add login, sessions, or tokens. Any future remote (non-loopback) access is explicitly opt-in (§8) and would require both `NEXUS42_DAEMON_API_KEY` and `NEXUS_DAEMON_REMOTE_BIND=1`; loopback remains the default.

> Implementation note for `daemon-api-surface-conventions.md`: the shared `ErrorResponse` (F-E1) is what the UI's toast/notification layer parses; the UI must never have to special-case per-handler error shapes.

### 4.3 CLI entry

See §11 and the [cli-spec.md](cli-spec.md) §6.3 amendment (proposed by this iteration): `nexus42 daemon start` serves the UI and logs its URL; an optional `nexus42 ui` convenience command may start the daemon (if not running) and open the OS browser. Final shape is a PM + architect decision; the spec records the chosen shape at P-last.

---

## 5. `tauri-api` adapter boundary (normative)

All daemon access from the UI goes through a single **`NexusClient`** interface. Core screen logic depends only on this interface, never on a concrete transport, so the same screens run unchanged in the browser today and inside a Tauri webview in V1.65.

```text
            ┌──────────────────────────────────────────┐
Control Room │  screen components → TanStack Query       │
 + Setup     │       → NexusClient (interface)           │
            └──────────────┬───────────────┬────────────┘
                           │               │
              BrowserClient│               │ TauriClient (V1.65 stub)
              fetch http://localhost:<port>│ invoke(...)
                           ▼               ▼
                   /v1/daemon/*  (axum, hardened by Track B)
```

- **`BrowserClient`** (V1.64) — `fetch` against `http://localhost:<port>/v1/daemon/*`. This is the only shipped impl this iteration.
- **`TauriClient`** (V1.65) — implements the same interface via Tauri `invoke`; ships as a stub/interface-reference now so the boundary is frozen and P2 screens are transport-agnostic. Not implemented in V1.64.

The interface exposes the Daemon API resources the MVP consumes (conceptual — exact signatures are owned by plan P1, compass §5 item #7):

| Resource group | Operations | MVP screen |
| --- | --- | --- |
| Works | list (cursor), get, create, patch, archive | Works dashboard; Work CRUD |
| Orchestration sessions | list, get (status) | Sessions view |
| Schedules | list (per Work), get | Schedule/cron view |
| Capabilities | list | Capability registry browser |
| Findings | list (per Work) | Findings view |
| Presets | list, get, create, update, delete, **validate** | Preset management CRUD |

**Invariant:** screens must not call `fetch`/`invoke` directly; they call `NexusClient`. This is what makes the V1.65 Tauri swap a one-impl change rather than a rewrite.

---

## 6. MVP surface (Control Room + Setup)

Seven screen groups. READ = visibility; CRUD = write/setup.

### 6.1 Control Room (READ-heavy — visibility)

| # | Screen group | Purpose | Mode |
| --- | --- | --- | --- |
| 1 | **Works dashboard** | List Works (cursor-paginated after F-P1) with status + completion %; drill into a Work detail view (intake status, stage, world binding, linked schedules). | READ |
| 2 | **Orchestration sessions** | List sessions with per-session status (running / completed / failed); watch what the runtime is doing right now. | READ |
| 3 | **Schedule / cron** | List cron roles per Work with next-fire in UTC and local time (parity with CLI `creator works cron`). Editing cron is deferred (§8). | READ |
| 4 | **Capability registry browser** | List `nexus.*` capabilities with descriptions — surfaces the V1.34 agent tool bridge so authors can see what the runtime can do. | READ |
| 5 | **Findings** | List findings (per Work, post-F-P2 endpoint) with status / severity filtering. Remediation actions are deferred (§8). | READ |

### 6.2 Setup (writes — configure the starting point)

| # | Screen group | Purpose | Mode |
| --- | --- | --- | --- |
| 6 | **Work CRUD** | Create / patch (status, stage) / archive a Work. Foundational for any authoring journey; mirrors `creator works` CLI. | CRUD |
| 7 | **Preset management** | Full CRUD on presets — list / show / create / update / delete / **validate** (dry-run). Schemas were promoted to codegen-ready in V1.63; this is their first real consumer. | CRUD |

The **validate (dry-run)** action within preset management is the highest-trust feature for a non-CLI author: it tells them a preset is safe to run before they commit. It is product-priority #1 within the Setup surface.

---

## 7. User stories

Each MVP screen group framed for the author persona (a writer, not an engineer) and the operator persona (configuring the runtime).

- **Works dashboard (READ)** — *As an author*, I want to open a page and see all my Works, their status, and how far along each one is, so I can pick up where I left up without recalling CLI commands.
- **Work detail (READ)** — *As an author*, I want to drill into one Work and see its intake status, current stage, and linked schedules, so I understand where it is in the journey.
- **Orchestration sessions (READ)** — *As an author*, I want to see whether the run I kicked off is still going, finished, or failed, so I know when it is safe to continue.
- **Schedule / cron (READ)** — *As an author/operator*, I want to see what is scheduled to fire next for each Work and when (in my local time), so I am not surprised by an automated run.
- **Capability registry browser (READ)** — *As an author*, I want to see what capabilities the runtime exposes, so I understand what my presets can invoke.
- **Findings (READ)** — *As an author*, I want to see the findings raised against a Work and how severe they are, so I can decide what to address next.
- **Work CRUD (Setup)** — *As an author*, I want to create a new Work, change its status, or archive one, so I can manage my creative efforts from the UI.
- **Preset management CRUD (Setup)** — *As an author/operator*, I want to list, inspect, edit, and delete presets — and validate one before running it — so I can configure what the runtime does without hand-editing YAML blind.

Common cross-cutting story: *As any user*, when a request fails I see one clear, actionable message (parsed from the shared `ErrorResponse`), not a raw stack trace or a per-endpoint error shape.

---

## 8. Non-goals (V1.64)

Explicitly deferred with durable tracking (compass §1.2 + §6; satisfies the Durable Roadmap Gate):

- **Tauri desktop shell (`apps/desktop`)** — V1.65. The SPA is Tauri-ready now; the shell wraps the same `apps/web/dist`.
- **Content-authoring UI** — chapter rich-text editor, outline editor, KB editor — V1.65+. CLI continues content production this iteration.
- **Findings-remediation UI** — **Ships in V1.77 (§23).** Findings were *visible* in V1.64; remediation (status transitions, `target_executor` assignment, inline edit) is the V1.77 lead surface. Remaining remediation follow-up (inline fix / re-run from finding) is deferred — **DR-28**.
- **Schedule / cron editor** — deferred. **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-27 (Control Room cron editing).
- **Mobile (Tauri v2 mobile targets)** — deferred. **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-61 (mobile / Tauri v2 mobile targets).
- **OpenAPI spec generation + generated TS client SDK (C2)** — deferred; TanStack Query + codegen TS types suffice for the SPA.
- **Remote (non-loopback) access / any new auth** — out of scope; would require its own auth spec.
- **agent-host sessions/operations/events(SSE) DTO promotion** — deferred. **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-57 (agent-host sessions/operations/events (SSE) DTO promotion).
- **F-P3 (rename list arrays to `items`)** + **F-F1 (`sort_by`/`sort_order`)** — adapter-covered in V1.64; structural closure V1.66+ — **(shipped V1.67 via convergence)**.

---

## 9. Roadmap

| Version | Scope |
| --- | --- |
| **V1.64** | Control Room + Setup MVP (browser SPA), daemon-served via `rust-embed`, `tauri-api` adapter boundary frozen. |
| **V1.65 (§13 stage)** | **Content-Authoring UI** (lead slice): outline rich-text editor + chapter structure table + structure CRUD + body read-only render + browser "Copy path"; **Track B** API hardening (chapter-content surface, `work_profile`, preset full CRUD, `items`+cursor). Tauri shell deliberately deferred to V1.66 (compass §0 Q1/Q5). |
| **V1.66** | **Tauri desktop shell** (`apps/desktop`) — loads `apps/web/dist`, system webview, `TauriClient` impl, daemon hosting (sidecar `nexus42 daemon start`); per-OS webview deps / signing / CI matrix. **"Open with" / "Reveal in file manager"** desktop integration (Tauri `shell.open`/`openWith`). Body full-text editor direction **rejected** (2026-06-26 — see §15.3); UI productivity deferred to V1.68. |
| **V1.67 (§15 stage)** | **Surface Convergence & De-risk**: Daemon API `items` array-key convergence + error-envelope consolidation (FE1-ORCH) + error-code casing + sort params (all transparent to the author); work_profile selector in Create-Work dialog; preset **TS-client promotion** (preset **management UI deferred to V1.68 canvas**). **Canvas Strategy Surface Exploration** authored (de-risk V1.68). Body editor direction **rejected** (§15.3). |
| **V1.69** | **Design System Maturation & Canvas Draft** (calm hygiene + V1.70 de-risk; no new UI screens, no wire change): `apps/web/DESIGN.md` migrated to **Production** completeness (YAML frontmatter SSOT + new `apps/web/DESIGN.dark.md` + body reshaped to rule-type docs + Level 3); **Canvas Exploration → Draft** (interface contracts + structured write-boundary + canvas→DESIGN.md token contract); 4 V1.67 frontend refactor residuals closed (work_profile literal union, `WORK_PROFILES` SSOT module, adapter-contract parity, preset query keys). Token names preserved verbatim → zero `index.css`/`tailwind.config.ts` consumer changes. |
| **V1.70 (§16 stage)** | **Canvas Strategy Implement (α)** — the first canvas surface ships: shared Canvas Shell (`@xyflow/react`, route-split behind `/strategy`) + Strategy (Preset) graph read projection (preset YAML → outer state-machine nodes + inner-DAG sub-flows + Converge join nodes per Draft §3.2) + live execution overlay (session-level status, 5 s poll) + Idea-input affordance + Run/Resume/Steer verbs (reuse existing schedule/orchestration endpoints; `wire_contracts_changed: FALSE` — A5 verdict: option a, client-side YAML parse from existing `getPreset(id).yaml`; no new backend route). DESIGN.md canvas tokens filled with concrete light + dark values. Accessibility baseline (keyboard-first, non-spatial alt view, ARIA live-region summary, focus management). UI copy adopts **Strategy** terminology; persisted identifiers remain `preset`. **`R-V167PSEC-QC1-S-UNMOUNT`** closed (daemon-status-bar subscription-cleanup race fix alongside overlay work). **Parallel ops track**: `desktop-build.yml` PR path filter narrowed (Rust-only PRs skip the 75 min macOS packaging build; main + tag/release triggers retain full coverage); new `desktop-release.yml` for distributable artifacts; **`R-V167PSEC-QC1-S-CI-SETUO`** closed (`set -euo pipefail`); `ci.yml` untouched. |
| **V1.71 (§17 stage)** | **Canvas Strategy write-boundary β** — Strategy patch routes + graphRevision tracking + conflict modal UX + canvas-write DESIGN.md tokens. Desktop signing groundwork is companion ops scope; outline+timeline and World KB remain future surfaces. |
| **V1.72 (§18 stage)** | **Canvas Outline+Timeline β** — 3 patch routes (`outline.patch_structure` / `outline.patch_chapter` / `timeline.patch_event`) + outlineRevision frontmatter tracking + outline-flavored conflict modal UX (use `{node_label}` placeholder adapting to chapter/event/volume domain entity) + non-spatial alternate views (sortable chapter list + sortable timeline event list) + 8 outline/timeline canvas-write DESIGN.md tokens (`canvas-outline-volume-fill` + 4 chapter-card statuses + `canvas-outline-timeline-event-pin` + `canvas-outline-foreshadow-edge` + `canvas-outline-timeline-marker` + `canvas-outline-conflict-marker`) + atomic outline markdown persistence (body preserved under `RuntimeLockGuard`). **Body ownership invariant**: outline markdown body remains V1.65 editor-owned and is never overwritten by canvas writes. **Companion hygiene + release hardening**: per-inspector save split (R-V171P0-QC1-004 HIGH) + strategy-canvas.tsx 7-module split ≤200 lines (R-V171P0-QC1-006 MEDIUM) + desktop-release.yml signing workflow completion (keychain + notarize + staple + unsigned fallback on signing failure; R-V171-CI-RELEASE-WORKFLOW-INCOMPLETE MEDIUM) + CI setup composite action (R-V171-CI-WORKFLOW-SETUP-DEDUPE LOW). Cmd/Ctrl+S save-trigger replay fixed via `lastHandledTriggerRef` edge-trigger. |
| **V1.73 (§19 stage)** | **Canvas World KB β** — third canvas surface (after Strategy α/β + Outline+Timeline β): 2 structured patch routes (`world_kb.patch_entity` for entity title/body/aliases/block_type edits + `world_kb.promote_candidate` for adopt/reject/merge promotion state machine) + per-row OCC conflict detection (reuses `kb_key_blocks.revision` + `kb_extract_jobs.version`; 409 `WorldKbConflictError` + 422 `WorldKbValidationError`) + Canvas UI: World KB graph projection (entity nodes + promotion-state badges + source-anchor edges + computable badges) + entity inspector + promotion inspector + conflict modal (KB-flavored copy) + non-spatial alternate view (sortable entity list with virtualization) + backend prerequisite: promoted World KB read+write from CLI-direct-DB to 4 first-class Daemon API routes + `@42ch/nexus-contracts` 0.8.0 → 0.9.0 (additive World KB DTOs) + 17 `canvas-worldkb-*` DESIGN.md tokens. Track B companion: 4 outline β hardening (MEDIUM validation gaps) + outline-canvas.tsx split + 2 release-hardening items. |
| **V1.74 (§20 stage)** | **Canvas World KB Relationships β** — fourth Canvas World KB capability: typed relationship edges, `world_kb.patch_relationship`, relationship inspector, non-spatial relationship table, conflict modal reuse, and relationship DESIGN.md tokens. Hygiene slate cleared in the same iteration. |
| **V1.75 (§21 stage)** | **Canvas-Pivot** — the V1.65 §13 whole-document TipTap outline editor (`chapter-page.tsx` Outline tab + `usePutChapterOutline` PUT save path) is **retired**. The V1.72 node-granular canvas is now the **sole outline authoring surface**. The canvas chapter inspector gains outline-prose TipTap editing via a new `content` field on `outline.patch_chapter` (parity-close — the inspector replicates the V1.65 editor's rich-text capability: headings, bold, italic, lists, markdown round-trip via `tiptap-markdown`). The retired `chapter-page.tsx` morphs to a read-only body view + "Edit outline → Canvas" redirect CTA — the reading/preview value is preserved (body prose render, frontmatter metadata strip, Copy Path), and outline authoring is relocated (not lost) to the canvas. This is a pre-1.0 hard cutover: no dual-editor deprecation period; the pivot is a clean retire+replace. `wire_contracts_changed: TRUE` (`content` field + V1.65 PUT write route/DTO removal → `@42ch/nexus-contracts` 0.10.0 → 0.11.0). |
| **V1.76 (§22 stage)** | **World KB Relationship γ — auto-extraction + confidence** — completes the World KB relationship surface. **Extraction proposes relationships**: `nexus.llm.extract` emits relationship candidates (entity pairs + relation_type + confidence + source anchors) from chapter text, persisted behind a `needs_review=1` gate (`source='extraction'`); the canvas shows them in a **Suggested pane** (sortable by confidence, default high→low, per-row Promote/Delete + bulk Promote all). **Confidence-weighting UX**: graph edges render with stepped confidence bands (low <0.4: 1px/30%, mid 0.4–<0.7: 2px/60%, high ≥0.7: 3px/100%) consuming the shipped DESIGN.md `canvas-worldkb-relationship-confidence-*` tokens; confidence-band colored badges (red/amber/green, uniform 8px) appear on edge labels. **`needs_review` gate semantics**: extraction defaults to `needs_review=1`; GET graph defaults to excluding suggested rows (`?include_suggested=true` surfaces them); suggested edges render dashed (distinct from confirmed solid). **Curation**: promotion clears `needs_review` via the existing `world_kb.patch_relationship` route (`needs_review: false` on update); `source` stays read-only provenance. `wire_contracts_changed: TRUE` (additive `needs_review` + `source` + extraction DTO + `include_suggested` → `@42ch/nexus-contracts` 0.11.0 → 0.12.0). No new DESIGN.md colors — the stepped bands reuse shipped confidence tokens. Desktop distribution v2 actual signing rollout remains blocked on Apple Developer ID cert + notarization credentials; Mobile (Tauri v2 mobile targets) remains future scope. |
| **V1.77 (§23 stage)** | **Findings-Remediation UI — quality loop closure** — promotes the Control-Room findings view from read-only (V1.64) to a full remediation authoring surface: status transitions (6-state lifecycle, server-enforced adjacency), `target_executor` assignment, inline edit of title/description/severity/kind/rule_suggestion. Detail-panel + row-action hybrid layout. TanStack Query optimistic mutations with stale-count refresh. DESIGN.md tokens: 6 status badges + severity color reuse + triage chrome. Closes the §8 deferred "Findings-remediation UI" item. Cross-profile (DB + Daemon API already profile-agnostic). `wire_contracts_changed: FALSE` (findings schemas already shipped + codegen'd; V1.77 only consumes them). |
| **V1.78 (§24 stage)** | **Creator Memory Review-Loop UI — self-loop closure** — publishes the memory OSS schemas the runtime already serves but never contracted (`schemas/local-api/memory/`, `@42ch/nexus-contracts` 0.12.0 → 0.13.0 additive) and builds a creator-scoped Control-Room Memory page: pending-review list with live count badge + delete (cursor-paginated, optimistic), "Review & Summarize" CTA triggering `POST /memory/review` with processing state + result-counters toast (`promoted`/`fragmented`/`dropped`), and a read-only fragments browser with optional keyword filter. Detail-panel + row-action hybrid layout (mirrors V1.77 findings page). TanStack Query optimistic mutations; `createPendingReview` stays CLI/producer-only (session-end capture pipeline owns creation), mirroring V1.77's `createFinding` CLI-only decision. 13 DESIGN.md memory tokens (pending-count badge, 5 task-kind chips, fragment chrome, inspector chrome, review CTA, fragment-filter input). `wire_contracts_changed: TRUE` (additive — memory schemas net-new; handler behavior unchanged). |

The Tauri-ready boundary (§5) is what keeps the V1.66 shell a thin wrap rather than a rewrite. The V1.68 canvas adds new screens (graph surfaces) on the unchanged transport boundary — not a re-architecture.

---

## 10. Wire contracts note

This Feature line adds **no new wire schemas** of its own. It consumes the V1.63-promoted local-api schemas and the Track-B hardening of V1.64:

- Depends on **F-E1** (shared `ErrorResponse`) for unified UI error handling.
- Depends on **F-P1** (Works cursor pagination) for the dashboard list.
- Depends on **F-P2** (findings list endpoint) for the findings view.
- Adapts around **F-P3** (list-array naming) and **F-F1** (sort) client-side until V1.66+.

Versioning, npm/Rust bumps, and the single breaking shape change (Works list) are owned by compass §1.3 and `daemon-api-surface-conventions.md`.

---

## 11. CLI entry (summary; detail in cli-spec.md §6.3 amendment)

- `nexus42 daemon start` serves the UI at `http://localhost:<port>/` and **logs that URL** on startup.
- An optional `nexus42 ui` (alias `nexus42 web`) convenience command starts the daemon if not running and opens the OS browser. Whether it ships in V1.64 (P3) or is deferred is a PM decision grounded in cost; the spec records the outcome at P-last.

---

## 12. Acceptance (spec-level)

1. The UI is served from the `nexus42` binary (release) with no Node runtime requirement, and from the Vite dev server (dev) proxying `/v1/daemon/*`.
2. All seven MVP screen groups render and operate against the hardened Daemon API; no screen calls a transport directly (all via `NexusClient`).
3. The `tauri-api` adapter boundary is frozen: `BrowserClient` ships, `TauriClient` exists as a documented stub/interface reference.
4. Errors surface as one parsed `ErrorResponse` shape across all screens.
5. No cloud-product / platform-gated feature appears in the UI while `platform_integration = paused`.
6. The UI consumes `@42ch/nexus-contracts` via `workspace:*` with zero handwritten duplicate wire types.
7. V1.65 Tauri shell is achievable by implementing `TauriClient` and wrapping `apps/web/dist` — no screen rewrite.

---

## 13. Next stage — Content-Authoring UI (V1.65 lead slice)

> **V1.75 Canvas-Pivot note:** the V1.65 whole-document TipTap outline editor described in this section (the `chapter-page.tsx` Outline tab + `usePutChapterOutline` PUT save path) is **retired** as of V1.75 (§21). Outline authoring now happens exclusively on the V1.72 node-granular canvas; `chapter-page.tsx` is now a read-only body view + "Edit outline → Canvas" CTA. The chapter structure table + body read-only render behaviors below remain accurate; only the outline *editor surface* moved to the canvas. See §21 for the pivot stage.

V1.64 made the runtime **legible and configurable** (Control Room + Setup). V1.65 takes the next step: the UI becomes an **authoring entry surface** — authors can plan, review, and restructure chapter **outlines and structure** directly in the browser, with the chapter **body rendered read-only**. This is the single highest-leverage product-completeness move after V1.64: the runtime is feature-complete for writing and now UI-reachable, but the UI cannot yet *shape* the writing — only observe and configure it.

> **Scope and roadmap SSOT**: [v1.65/delivery-compass.md](../iterations/v1.65/delivery-compass.md) §0 (grill decisions) + §1.1 (Track A) + §1.2 (V1.66 roadmap) + §5 (open design items). This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 13.1 What ships in V1.65 (Track A lead slice)

The browser SPA gains an authoring surface layered on the V1.64 Control Room + Setup screens. All new screens route through the same `NexusClient` interface (§5) and consume the new V1.65 chapter-content Daemon API (Track B / P0 backend; conventions in [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md)).

- **Chapter structure table** (per-Work, multi-Work switcher reusing the V1.64 Works dashboard entry): columns — chapter #, title (**display-only** — derived from outline frontmatter or slug/chapter# fallback; no `title` column exists in `work_chapters` in V1.65), slug, planned word count, volume, status (`not_started` / `outlined` / `draft` / `finalized` / `published`), actual word count. Sortable by chapter #.
- **Outline rich-text editor**: edit a chapter's `outline_path` markdown in a rich-text editor; save writes the file atomically (reuse the reconcile atomic-write pattern) and updates DB metadata (`outline_path`, `updated_at`) in the same transaction. Restricted to a markdown subset (headings, lists, bold/italic, code, blockquote, links).
- **Structure CRUD**: edit slug / planned word count / volume; advance status `not_started → outlined` (reverse transitions gated). (`title` is display-only in V1.65 — no DB column; title authoring happens in the outline editor; a `title`-column migration + title CRUD is deferred to V1.66.) `finalized` / `published` chapters are protected: structural edits require a confirmation dialog; **deletion is hard-blocked**.
- **Body read-only rendering**: render a chapter's `body_path` markdown (frontmatter-aware — surface status/metadata in a read-only header strip, render body prose read-only). Right-click context menu offers **"Copy path"** only (browser clipboard write; path sourced from the API).
- **Soft concurrency** (compass §0 Q2/Q3): no hard lock. The outline editor shows a non-blocking but unmissable persistent banner when editing the outline of a chapter already in `draft` or `finalized` status. The banner states plainly: editing the outline will **not** re-draft the body, and guides the author to the explicit next step — reverse-transition the chapter status to `outlined` (then advance to `draft`) via structure-CRUD to trigger a re-draft. Orchestration reads the outline at draft-time (a natural snapshot of whatever is on disk).

### 13.2 The authoring loop this enables

The UI closes the **plan / review / restructure** loop for an author who is not terminal-fluent:

1. **Plan** — draft and revise a chapter's outline in rich text; the outline is the author-facing planning document that orchestration reads to draft body prose.
2. **Review** — read a chapter's rendered body read-only; copy its file path to open it in the author's own editor.
3. **Restructure** — fix slugs, volumes, planned word counts; advance a chapter from `not_started` to `outlined` once its outline is ready. (Title text is shaped in the outline editor, where the chapter heading naturally lives.)

**The CLI still owns body drafting.** Body prose is written by the orchestration engine through the V1.34 host-tool bridge; V1.65 gives the UI no body write path (see §13.3). The UI is the *planning and structure* surface; the CLI/runtime remains the *drafting* surface until V1.66.

### 13.3 Non-goals for V1.65 (durable V1.66 roadmap)

Explicitly deferred with rationale (compass §0 Q2/Q4/Q5, §1.2; satisfies the Durable Roadmap Gate):

- **Body full-text editor (`body_path` write)** — V1.66. Requires a per-chapter edit-lock design (UI claims chapter N → orchestration skips/queues; lock-expiry policy), MD↔rich-text lossless round-trip, frontmatter/status sync, and a conflict policy with the orchestration co-writer. Lands only after the lock design is reviewed.
- **"Open with" / "Reveal in file manager" right-click actions** — V1.66 Tauri desktop shell. Launching an OS process to open a file is a **native-shell** capability (Tauri `shell.open` / `openWith` with a scope whitelist), **not** a Web daemon responsibility (compass §0 Q5). The browser sandbox has no such capability; making the daemon a "process launcher" would be the wrong layering. V1.65 ships "Copy path" only.
- **Tauri desktop shell (`apps/desktop`)** — V1.66. The SPA is Tauri-ready now (§5 adapter boundary; no browser-only APIs in editor core); the shell wraps the same `apps/web/dist`.
- **Drag-to-reorder chapters / bulk chapter operations / manual reconcile trigger / outline template library** — V1.66+.

### 13.4 User stories (V1.65 slice)

- **Outline editor** — *As an author*, I can open a chapter and edit its outline in a rich-text editor, then save it back as markdown, so I can plan the chapter's shape without dropping into the terminal.
- **Structure CRUD** — *As an author*, I can fix a chapter's slug, planned word count, and volume, and advance its status from `not_started` to `outlined`, so the structure of my Work reflects my plan.
- **Protected edits** — *As an author*, when I edit the structure of a `finalized` or `published` chapter the UI asks me to confirm, and it refuses to delete one, so I cannot accidentally destroy settled work.
- **Body read + copy path** — *As an author*, I can read a chapter's rendered body and copy its file path, so I can open it in my own editor to read or annotate.
- **Soft-concurrency awareness** — *As an author*, when I edit the outline of a chapter that is already drafted, the UI tells me plainly that my change will not re-draft the body and shows me the explicit next step (reverse the chapter status to `outlined` to trigger a re-draft), so I am not left waiting or surprised.
- **Multi-Work navigation** — *As an author*, I can switch between my Works while planning, so I can keep several projects in flight from one window.

### 13.5 Wire contracts (V1.65)

The authoring surface consumes new chapter-content schemas (additive, owned by Track B / P0; conventions in [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md)): chapter list (cursor + `items`) / detail / outline GET+PUT (atomic write) / structure PATCH (status progression) / body GET (read-only), plus `work_profile` on Work requests and full preset CRUD routes. Versioning, npm/Rust bumps, and per-DTO `schema_version` increments are owned by compass §1.3.

---

*Local-first Web UI product contract. V1.64 Shipped (Control Room + Setup); V1.65 §13 Content-Authoring stage amendment promotes at V1.65 P-last. Design tokens: `apps/web/DESIGN.md` (V1.64 Standard + V1.65 Standard+ editor/table/context-menu increment); design intent input: [web-ui-design-requirements.md](web-ui-design-requirements.md).*

---

## 14. Next stage — Desktop Shell (V1.66 lead slice)

V1.65 made the UI an **authoring entry surface** in the browser. V1.66 takes Nexus from **"open a browser tab to `localhost:8420`"** to a **double-clickable macOS desktop application**. The browser SPA transport stays **unchanged** (screen data access remains transport-agnostic); a new `apps/desktop` Tauri v2 wrapper loads the `apps/web` dist, the `TauriClient` impl of `NexusClient` swaps in, and the bundled `nexus42` daemon comes up transparently on launch. This is the gating prerequisite for everything desktop-native in the roadmap (signing, multi-OS, auto-update, mobile).

> **Scope and roadmap SSOT**: [v1.66/delivery-compass.md](../iterations/v1.66/delivery-compass.md) §0 (grill decisions Q1/Q2) + §1.1 (Track A) + §1.2 (V1.67+ roadmap) + §5 (locked design items). Contract detail: [desktop-shell.md](desktop-shell.md). This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 14.1 What ships in V1.66 (Track A lead slice)

A Tauri v2 desktop wrapper layered around the transport-unchanged V1.65 SPA, plus the desktop-only `NexusClient` surface the browser sandbox cannot provide.

- **`apps/desktop` Tauri v2 app** (new pnpm workspace member under `apps/*`): `tauri.conf.json` (productName, macOS bundle id, window config, `build.frontendDist` = bundled `apps/web` dist); Rust `src-tauri/` (Tauri app entry, plugin registration — `opener`, `shell`; NO `http` plugin — webview fetches loopback directly). **macOS-only target in V1.66** (`aarch64-apple-darwin` + `x86_64-apple-darwin`, universal). Windows/Linux deferred (V1.67+).
- **`TauriClient` impl** (replaces the V1.65 stub at `apps/web/src/lib/nexus/tauri-client.ts`): implements the full **21-method** `NexusClient` interface as **thin desktop-augmentation over `BrowserClient`** (compass §5 #1 LOCKED) — data methods reuse the identical HTTP transport to the localhost daemon; `TauriClient` adds only the desktop-only methods below. V1.64/V1.65 HTTP work reused wholesale.
- **Desktop-only `NexusClient` extensions** (the new surface): `openWith(path)` / `revealInFinder(path)` (Tauri custom commands → `plugin-opener`; runtime workspace-root path guard, §14.6), plus **daemon lifecycle** (`getDaemonStatus` / `startDaemon` / `stopDaemon`). Exposed via the interface **only in desktop mode** (capability detection: `NEXUS_DESKTOP` flag + `isTauri`, checked once at the client factory).
- **Q5 desktop actions — right-click context menu**: on the chapter body read-only view + outline editor surfaces (V1.65), wire "Copy path" (browser + desktop) + **"Open with…"** (system MD-editor picker; desktop only) + **"Reveal in Finder"** (desktop only). Browser build keeps "Copy path" only — **no greyed-out teasing** entries.
- **Bundled `nexus42` sidecar** (transparent daemon autostart): Tauri `externalBin` + `plugin-shell` Sidecar (compass §5 #2 LOCKED); auto-start on launch, stop on quit, health probe + restart-on-crash. The user double-clicks the `.app` and the daemon comes up — **no terminal**. In-process lib link deferred V1.67+.
- **macOS CI unsigned `.app` build leg**: `desktop-build` workflow job (unsigned `.app` + `.dmg` artifacts uploaded). **No signing/notarization/auto-update/GitHub Releases** in V1.66.

**Stage status**: **Shipped (V1.66)** — QC tri-review Approve (after fix-wave-1: port-exposure-to-SPA + attached-daemon-probe + dev-prereq docs + CI cache/path-filter + error-label split) + QA Pass.

### 14.2 The desktop loop this enables

1. **Launch** — double-click the `.app`; the window opens to the Control Room and the daemon starts transparently (no terminal, no port to remember).
2. **Work** — use the full V1.65 surface (Control Room + Setup + Outline/Structure Authoring) exactly as in the browser — same screens, same transport contracts.
3. **Reach the file** — right-click a chapter body or outline path → "Open with…" to pick a system markdown editor, or "Reveal in Finder" to jump to the file. Transparent daemon autostart is the larger *invisible* win; open-with/reveal is the one new *visible* capability.

### 14.3 Non-goals for V1.66 (durable V1.67+ roadmap)

- **Body full-text editor + per-chapter edit lock** — **rejected** (2026-06-26 V1.67 re-discussion). Nexus is an AI-autonomous executor — the AI owns prose; a manual rich-text body editor is the wrong direction. The V1.68 lead is the **Canvas Strategy Surface**. See §15.3.
- **UI productivity wave** — **V1.67**. Drag-reorder, bulk ops, reconcile trigger, outline templates.
- **Windows + Linux + signing + notarization + GitHub Releases + auto-update** — **V1.67+**. The unsigned `.app` is the V1.66 deliverable. (Until signing lands, the no-Gatekeeper-friction author win is not realized — V1.66's user is the developer/contributor; see §14.4.)
- **In-process `nexus-daemon-runtime` lib link; system tray / menu-bar / hotkeys / notifications; mobile** — **V1.67+ / post-V1.67**.

### 14.4 User stories (V1.66 slice)

- **One-click launch** — *As an author*, I double-click the Nexus app and the Control Room opens with the daemon already running, so I never open a terminal or remember a port.
- **Native file actions** — *As an author*, I right-click a chapter and choose "Open with…" to edit in my own editor, or "Reveal in Finder" to see the file.
- **Daemon visibility** — *As an author*, I see at a glance whether the daemon is healthy (and am told plainly, with a next step, if it could not start — e.g., port in use).
- **Browser parity** — *As an author*, everything from the browser tab works identically in the desktop app — strict superset, not a different product.
- **Contributor install (V1.66 reality)** — *As a developer/contributor*, I pull the unsigned `.app`/`.dmg` from CI and run it locally (bypassing Gatekeeper once) to exercise the full desktop stack before signing lands in V1.67+.

### 14.5 Wire contracts (V1.66)

**No new wire schemas** (`wire_contracts_changed: false`, confirmed Phase 2b). The shell is a packaging/delivery layer: `TauriClient` reuses the identical HTTP transport; desktop-only methods are Tauri IPC; the 3 residuals are test/refactor/hardening. `@42ch/nexus-contracts` version unaffected.

### 14.6 Capability table delta (desktop-only `NexusClient` extensions)

| Method | Mode | Transport | Notes |
| --- | --- | --- | --- |
| `openWith(path)` | desktop only | Tauri custom command → `plugin-opener.openPath()` | Runtime path-guarded to active workspace root (W-002-equivalent; Tauri scope = defense-in-depth only). |
| `revealInFinder(path)` | desktop only | Tauri custom command → `plugin-opener.revealItemInDir()` | Same runtime path guard. |
| `getDaemonStatus()` | desktop only | Tauri `plugin-shell` / sidecar IPC | Health + port; drives the status indicator. |
| `startDaemon()` / `stopDaemon()` | desktop only | Tauri `plugin-shell` Sidecar | Lifecycle control; autostart on launch is default. |
| `copyPath(path)` | browser + desktop | clipboard write (V1.65 reuse) | Unchanged. |

All other `NexusClient` methods = identical HTTP transport to the localhost daemon (reuse of V1.64/V1.65 `BrowserClient` paths). Detail: [desktop-shell.md](desktop-shell.md).

---

## 15. Next stage — Surface Convergence & De-risk (V1.67)

V1.66 shipped the Tauri desktop shell. V1.67 is a **hygiene-lead consolidation & de-risk** iteration: it converges the Daemon API surface to one error envelope + one array-key convention + casing discipline, closes ~26 residuals, polishes the just-shipped desktop shell, closes the work-profile selector gap, and authors the **Canvas Strategy Surface Exploration** that de-risks the V1.68 lead. **No new author-facing features ship** — the only user-visible change is a work-profile selector in the Create-Work dialog. The canvas *implement* is V1.68; V1.67 authors its *design* only (see §15.3). *(Revised 2026-06-26: the prior body-editor lead was rejected — Nexus is an AI-autonomous executor; the AI owns prose, the human steers via Canvas.)*

> **Scope and roadmap SSOT**: [v1.67/delivery-compass.md](../iterations/v1.67/delivery-compass.md) §0 (grill decisions + 2026-06-26 re-discussion Q4–Q6) + §1.1 (Tracks A–F) + §1.2 (V1.68 roadmap) + §5 (locked design items). This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 15.1 What ships in V1.67 (author-visible surface)

One small but unlocking UI change, a transport-only client promotion, plus a transparent API convergence the author never sees.

- **Work-profile selector in the Create-Work dialog** (G1): when an author creates a new Work, the dialog now includes a profile selector offering `novel`, `essay`, `game-bible`, and `script`. The wire contract already carried `work_profile` since V1.65 (additive optional field); V1.67 surfaces it in the UI. An author who skips the selector gets the default (`novel`) — no behavior change from V1.66. This is a prerequisite for the V1.68 canvas, which will tailor the steering surface per profile.
- **Preset CRUD TS-client promotion** (G2, transport half only): the daemon already ships `GET/PATCH/DELETE /v1/local/presets/{id}` + contracts; V1.67 promotes the 3 methods on the TS `NexusClient` interface (21 → 24) so the V1.68 canvas Strategy surface can consume them. **No form-based preset-management UI is built in V1.67** — the canvas Strategy surface supersedes a form UI (§0 Q6); building one now would be throwaway.
- **`items` array-key convergence** (transparent to authors): all schema-backed list responses now use `items` (previously `works`/`schedules`/`sessions`/`capabilities`). Pre-1.0 breaking wire change (see §15.5), but the author experiences nothing — the UI data layer adapts internally.

### 15.2 The de-risk loop this enables

V1.67 does not add an authoring loop; it *unblocks* the V1.68 canvas loop and *cleans* the foundation it builds on:

1. **Profile up-front** — an author starting a Work picks its profile at creation time, so the V1.68 canvas can tailor the steering surface per profile without a retrofit.
2. **Preset transport ready** — the TS client can already get/update/delete presets, so the V1.68 canvas Strategy editor wires directly to the daemon surface (no transport gap to close mid-canvas-build).
3. **Consistent API surface** — the V1.68 canvas (a heavy Daemon API consumer — graph nodes bind to lists/details) builds on a Daemon API with one error envelope, one array-key convention, and casing discipline — not the ad-hoc shapes V1.64 left behind.

### 15.3 Non-goals for V1.67 (durable V1.68 roadmap)

Explicitly deferred with rationale (compass §0 Q2/Q3, §1.2; satisfies the Durable Roadmap Gate):

- **Canvas Strategy Surface *implement*** — **V1.68 lead** (revised 2026-06-26; replaces the rejected body-editor lead). V1.67 ships the *Exploration* only ([canvas-strategy-surface.md](canvas-strategy-surface.md)): product thesis (Nexus = AI-autonomous executor; human inputs Idea + steers via Canvas; AI owns prose) + 3 canvas surfaces (Strategy/outline+timeline/World KB) on React Flow + no-raw-file-editing principle + TipTap-as-in-node. V1.68 promotes Exploration → implement.
- **Body full-text editor** — **rejected direction** (2026-06-26 product-vision correction). Nexus is an AI-autonomous executor; the AI owns prose. `body-editor.md` archived. The shipped V1.65 whole-document outline editor's canvas-pivot is part of V1.68 canvas work.
- **Preset-management form UI** (`R-V164-P2-G2` UI half) — **V1.68 canvas** (the canvas Strategy surface supersedes a form UI; the TS-client transport half ships in V1.67).
- **UI productivity wave** — **V1.68** (largely subsumed by the canvas graph model; re-evaluate at V1.68 Prepare). Drag-reorder, bulk ops, reconcile trigger, outline templates.
- **Desktop distribution v2** — **V1.68 (or its own iteration)**. Windows + Linux + signing + notarization + auto-update + in-process lib link. Decision point at V1.67 P-last.
- **CapabilityInfo admission-gate UI** (`R-V164-P2-G3`) — V1.68.
- **Live served-UI smoke** (`R-V164-P2-S1`) — V1.68.
- **Chapter table virtualization** (`R-V165-QC3-VIRT`) — V1.68.
- **DX/UX polish grab-bag (UI subset)** (`R-V165-QC-SUGG-DX`) — V1.68.

### 15.4 User stories (V1.67 slice)

- **Work-profile selector** — *As an author*, when I create a new Work I can choose its profile (novel, essay, game-bible, or script) from a selector in the Create-Work dialog, so the runtime and the future canvas can tailor the steering experience to the kind of thing I am writing.

(The preset-management stories — inspect/edit/delete in a form UI — are **deferred to the V1.68 canvas Strategy surface**, where preset/strategy editing is a graph operation, not a form. V1.67 only makes the TS transport capable of those operations.)

### 15.5 Wire contracts (V1.67)

**`wire_contracts_changed: TRUE`** (`@42ch/nexus-contracts` 0.5.0 → 0.6.0; compass §1.3 + §5 LOCKED). Two breaking changes: F-P3 array-key rename → `items` (4 schema-backed endpoints) + error-code casing ratification (global UPPER→lower snake_case). F-F1 sort is additive; G1 is frontend-only; G2 is frontend-only (TS-client promotion; no UI). `pnpm run codegen` regenerates TS + Rust. The 2026-06-26 canvas re-discussion changes **no** wire contracts (canvas is V1.68 implement; V1.67 ships no canvas code).

---

*Local-first Web UI product contract. V1.64 Shipped (Control Room + Setup) → V1.65 §13 Content-Authoring → V1.66 §14 Desktop Shell → V1.67 §15 Surface Convergence & De-risk → V1.69 Design System Maturation & Canvas Draft → V1.70 §16 Canvas Strategy Implement (α) + CI/desktop-build optimization → V1.71 §17 Canvas Strategy Write-Boundary (β) → V1.72+ outline/timeline + World KB. Design tokens: `apps/web/DESIGN.md` (V1.65 Standard+ + V1.66 desktop supplement + V1.69 Production migration + V1.70 canvas-token fill + V1.71 canvas-write tokens).*

---

## 16. Next stage — Canvas Strategy Implement (α) + CI/desktop-build optimization (V1.70)

V1.69 shipped the **Canvas Strategy Surface Draft** (interface contracts + structured write-boundary + canvas→DESIGN.md token contract) and the Design System Production migration. V1.70 turns the Draft into the **first human-steerable Canvas surface** at α depth, and runs a parallel **CI/desktop-build optimization** ops track that unblocks the PR feedback loop (Rust-only PRs stop triggering a 75 min macOS packaging build; distributable release packages move to a release-gated workflow).

> **Scope and roadmap SSOT**: [v1.70/delivery-compass.md](../iterations/v1.70/delivery-compass.md) §0 (grill decisions Q1–Q3) + §1.1 (Tracks A+B) + §1.2 (V1.71+ roadmap) + §5 (locked design items). This section records the product contract; the compass is authoritative for scope, batching, and residual tracking. Architectural detail: [canvas-strategy-surface.md](canvas-strategy-surface.md) (V1.70 Shipped α — write-boundary + outline/timeline + World KB remain Draft V1.71+).

### 16.1 What ships in V1.70 (Track A — Canvas α)

The **Strategy (Preset) surface** ships at **α depth** — read + visualization + live overlay + Idea-steer. The human *sees* the Strategy as a graph and *steers* execution with an Idea; the AI owns prose.

- **Shared Canvas Shell** (`@xyflow/react`, route-split behind `/strategy`): React Flow provider, pan/zoom, minimap, dot-grid, selection model, side inspector, validation panel (read-only), keyboard shortcuts, screen-reader graph summary, `NexusClient` transport injection. **Route-split verified**: `strategy-page-*.js` is a separate 305 kB chunk; React Flow is excluded from the Control Room bootstrap.
- **Strategy graph adapter (read projection)**: preset YAML → React Flow `nodes`/`edges` per Draft §3.2 — outer state-machine states → top-level nodes; `inner_graph` states → group/sub-flow nodes (`parentId` + `extent:parent`); Converge merge-point states → join nodes (`wait_for_all` / `first_completed` / `any`); linear `next` / labeled `branches` / default → edges with condition labels. **10 unit tests** in `strategy-graph.test.ts` cover the Draft §3.2 mapping.
- **Live execution overlay (bounded)**: session `current_task_id` + `status` → node highlight + status ring, polled at 5 s. **Bounded to session-level per A5 verdict** — completed-path history + child-session hierarchy are V1.71.
- **Idea-input affordance + Run/Resume/Steer**: persistent canvas affordance for Idea input (global entry + contextual node action). Submitting an Idea enqueues/resumes via **existing** `addSchedule` / `editCoreContext` + `signalSchedule` (promoted onto `NexusClient`, V1.67 G2 pattern). Verbs: **Steer / Run / Resume / Ask Nexus to revise**. Idea submissions land as visible steering artifacts.
- **Canvas → DESIGN.md token fill**: the 11 LEVEL placeholder canvas tokens (`canvas-surface`, `canvas-grid`, `canvas-node-fill/-hover/-border/-border-selected`, `canvas-edge/-hover`, `canvas-port`, `canvas-minimap`, `canvas-strategy-accent`) filled with concrete light + dark values in `apps/web/DESIGN.md` + `apps/web/DESIGN.dark.md`. Token *names* preserved (V1.69 invariant continues). `canvas-strategy-accent` derives from the purple family.
- **Accessibility baseline**: keyboard-focusable nodes/edges, non-spatial alternate view (Strategy states in execution-order list + transition table), ARIA live-region graph summary, focus-visible rings, read-only inspector. Closes the Draft §4.4 a11y requirements as product requirements, not just tech checkboxes.

### 16.2 The steering loop this enables

V1.70 does not ship a full authoring loop; it ships the **steering surface** that V1.71 will make editable:

1. **Read the Strategy as a graph** — *As an author*, I see my Strategy (preset) rendered as a state-machine graph with visible join/wait nodes, so I understand how Nexus will execute my Work before it runs.
2. **Steer by Idea** — *As an author*, I express an Idea (Work-level or on a specific node) and choose **Steer / Run / Resume / Ask Nexus to revise**, then Nexus executes — drafting prose, advancing the chapter, updating the KB — so I direct the work without typing the body myself.
3. **Review AI execution on the canvas** — *As an author*, after Nexus executes, I see what changed on the canvas (node status, generated-output links, pending instructions) and review the result read-only, so I stay in command of an autonomous process.

(The outline+timeline and World KB surfaces, plus structured node-granular *edits* of the Strategy graph, are V1.71+. V1.70 is read + overlay + Idea-steer only.)

### 16.3 Parallel ops track — CI/desktop-build optimization (Track B)

The desktop packaging CI was wasteful on PRs: any `crates/**` change triggered a 75 min macOS Tauri universal build, even though sidecar compilation is already covered by `ci.yml` (clippy + rust-tests on ubuntu). V1.70 fixes this without changing the test gate:

- **`desktop-build.yml` PR path filter narrowed** to `apps/desktop/**`, `apps/web/**`, `.github/workflows/**` — Rust-only PRs no longer trigger the macOS packaging build. `push:main` retains broad coverage as the integration safety net.
- **New `desktop-release.yml`** triggers on `release.published` only (single-source per release; QC3 W1 double-run fix), produces distributable `.app.zip` + `.dmg` artifacts for GitHub Releases.
- **`set -euo pipefail`** added to desktop-build universal + fallback blocks (closes `R-V167PSEC-QC1-S-CI-SETUO`).
- **`ci.yml` untouched** — the test gate stays as-is.

### 16.4 Wire contracts (V1.70)

**`wire_contracts_changed: FALSE`** (LOCKED by PM; `@architect` Phase 2b countersigned). No schemas/codegen/`@42ch/nexus-contracts`/DTO change. The Idea-steer path explicitly reuses existing schedule input/core-context/signal surfaces. The A5 read-endpoint question (Draft §3.2: "promote read endpoints such as 'get Strategy graph projection' if existing endpoints are too YAML/raw") was **resolved in favor of option (a)**: `GET /v1/local/presets/{id}` returns `GetPresetResponse { id, source, path?, yaml }` sufficient for client-side Strategy graph projection; `GET /v1/local/orchestration/sessions/{session_id}` returns `SessionDetailResponse { session: SessionSummary }` bounding the V1.70 live overlay to current-node/status highlighting plus session-level state. Rich overlay data (completed-path history, child-session hierarchy) is deferred to the V1.71 write-boundary/overlay-contract plan rather than adding a V1.70 read route.

### 16.5 Non-goals for V1.70 (durable V1.71+ roadmap)

Explicitly deferred with rationale (compass §1.2; satisfies the Durable Roadmap Gate):

- **Structured node-granular *edits*** — rename state, rewire edge, patch prompt template (Draft §3.5 operation DTOs) — **V1.71**. V1.70 Strategy canvas is read + overlay + Idea-steer only.
- **Canvas outline+timeline surface** (Draft §3.3 surface 2) — **V1.71+**.
- **Canvas World KB surface** (Draft §3.3 surface 3) — **V1.71+**.
- **CLI / schema rename of `preset` → `strategy`** — breaking; deferred. V1.70 adopts **Strategy** terminology in UI copy only (Draft §4.2). Tracked: **DF-V1122-HARNESS-RENAME** ([tracker §2.3](../knowledge/deferred-features-cross-version-tracker.md)).
- **Desktop distribution v2** (signing / tri-OS / auto-update) — deferred. **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-58 (desktop distribution v2).
- **Rich live overlay** (completed-path history, child-session hierarchy) — deferred. **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-29 (rich live overlay).
- **Tauri WKWebView runtime smoke** — implementer documented they could not run Tauri locally; QA ran `cargo check` only. Full gesture/keyboard/pan-zoom validation inside actual WKWebView is a V1.71 follow-up if deeper runtime validation is needed.

### 16.6 User stories (V1.70 α slice)

- **Read the Strategy as a graph** — *As an author*, I see my Strategy (preset) rendered as a state-machine graph with visible join/wait nodes, so I understand how Nexus will execute my Work before it runs — and I can rewire a branch or adjust a gate on the canvas *(rewire/edit deferred to V1.71; V1.70 ships the read + overlay)*.
- **Steer by Idea** — *As an author*, I express an Idea (Work-level or on a specific node) and choose **Steer / Run / Resume / Ask Nexus to revise**, then Nexus executes — drafting prose, advancing the chapter, updating the KB — so I direct the work without typing the body myself.

(Outline chapters, World KB entities, and non-Strategy graph editing live in V1.72+ stories.)

---

## 17. Next stage — Canvas Strategy Write-Boundary (V1.71)

V1.70 made the Strategy canvas legible and steerable. V1.71 makes the **Strategy surface editable at node granularity** while preserving the core boundary: the browser/Tauri webview never writes raw files. All Strategy edits flow through schema-backed Daemon API patch routes, daemon validation, atomic persistence, and graphRevision conflict handling.

> **Scope and roadmap SSOT**: [v1.71/delivery-compass.md](../iterations/v1.71/delivery-compass.md) §1.1 Track A (A1–A9), §1.3 wire contracts, §2 normative specs, and §6 risk notes. Architectural detail: [canvas-strategy-surface.md](canvas-strategy-surface.md) (V1.71 Shipped β) and [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md) §7 patch-route pattern.

### 17.1 What ships in V1.71 (Track A — Strategy β writes)

- **Strategy patch routes**: the UI calls three new Daemon API routes through `NexusClient`, not `fetch`/Tauri filesystem access:
  - `POST /v1/local/strategies/{strategy_id}/states/{state_id}/patch` (`StrategyPatchStateRequest` → `StrategyPatchResponse`) for state label/description edits.
  - `POST /v1/local/strategies/{strategy_id}/transitions/patch` (`StrategyPatchTransitionRequest` → `StrategyPatchResponse`) for edge/transition condition and target rewiring.
  - `POST /v1/local/strategies/{strategy_id}/states/{state_id}/prompt/patch` (`StrategyPatchPromptTemplateRequest` → `StrategyPatchResponse`) for in-node prompt-template edits.
- **Conflict modal copy/flow**: stale writes return 409 `StrategyConflictError` with the current revision and structured locator. The canvas keeps the user's draft patch, refetches the canonical Strategy, and presents a modal with:
  - **Headline**: "This node changed while you were editing."
  - **Body**: "Nexus updated **{node label}** to revision **{current_revision}** while you were editing **{field}**. Your change is still in the inspector."
  - **What changed**: the canonical value that now differs from the user's last known revision.
  - **What you were about to do**: the user's draft value for the same path.
  - **Actions**: **Use current** (primary/default), **Reapply my edit**, and **Review side-by-side** (enabled only when draft and canonical changes touch non-overlapping fields; disabled for same-field/path or prompt-template conflicts). Cancel returns focus to the originating inspector.
- **Canvas inspector copy**: state inspector header "Edit state" with fields "Label" and "Description"; edge inspector header "Edit transition" with fields "Condition" and "Target state"; prompt-template node header "Edit prompt"; inline validation errors and a save-in-progress indicator; a 409 surfaces the conflict modal instead of a generic error.
- **graphRevision tracking + freshness indicator**: the client tracks `lastKnownRevision` per Strategy; the daemon stores the canonical revision as `revision:` in the preset YAML header. Existing presets without the key read as revision `0`; the first accepted patch writes `revision: 1`. The canvas chrome or command palette shows "Strategy · revision {revision} · updated {relative time}" with "Refresh now"; when a newer revision exists it shows "Strategy updated elsewhere · revision {newer} available · Refresh to see latest".
- **Canvas-write DESIGN.md tokens**: V1.71 adds concrete light/dark tokens for write-mode states (selected/focus border, save-in-progress, conflict marker) while preserving the V1.69/V1.70 token-name invariant.

### 17.2 The editing loop this enables

1. **Inspect** — the author selects a Strategy state, edge, or prompt-template node and edits only that structured node/subresource.
2. **Validate** — the daemon verifies ids, reachability, condition syntax, prompt-template references, and revision preconditions before accepting the patch.
3. **Commit or resolve** — successful patches return a new revision and canonical state; conflicts present current-vs-draft recovery instead of silently overwriting newer daemon/orchestration changes.

### 17.3 Non-goals for V1.71

- **No outline+timeline canvas write surface** — V1.72 candidate using the same patch-route convention after its DTOs and validators are promoted.
- **No World KB canvas write surface** — V1.72+ candidate using the same patch-route convention after promotion/adoption/relationship DTOs are promoted.
- **No CLI/schema rename of `preset` → `strategy`** — UI terminology remains Strategy, persisted identifiers and routes continue to expose `preset` where already shipped.
- **No removal or regression of the V1.65 outline editor** — TipTap is promoted for Strategy prompt nodes only; the historical outline editor remains intact until a future canvas-pivot plan retires it.

### 17.4 Wire contracts (V1.71)

**`wire_contracts_changed: TRUE`** (`@42ch/nexus-contracts` 0.6.0 → 0.7.0 by default). V1.71 promotes new Strategy patch DTOs and routes through schemas/codegen. The fallback to additive 0.6.1 is allowed only if downstream coordination rejects the pre-1.0 minor bump and the change remains strictly additive.

---

## 18. Next stage — Canvas Outline+Timeline β (V1.72)

V1.71 made the Strategy canvas editable at node granularity. V1.72 extends the canvas to the **Outline+Timeline surface** — the second of the three Draft canvas surfaces — bringing Work-structure (Volume → Chapter → Scene/Beat) and timeline events onto the graph with structured patch routes, outlineRevision conflict handling, and non-spatial alternate views.

> **Scope and roadmap SSOT**: [v1.72/delivery-compass.md](../iterations/v1.72/delivery-compass.md) §1.1 Tracks A+B, §1.3 wire contracts, §2 normative specs. This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 18.1 What ships in V1.72 (Track A — Outline+Timeline β)

- **3 outline/timeline patch routes**: the UI calls three new Daemon API routes through `NexusClient`, following the V1.71 Strategy patch-route convention:
  - `POST /v1/local/outline/patch_structure` for volume-level structure edits (order, title, metadata).
  - `POST /v1/local/outline/patch_chapter` for per-chapter edits (slug, planned word count, volume assignment, status advancement).
  - `POST /v1/local/timeline/patch_event` for timeline event CRUD (timestamp, description, linked chapters).
- **outlineRevision conflict policy**: stale writes return 409 with outlineRevision locator; conflict modal adapts the Strategy pattern (§17.1) with `{node_label}` placeholder substituting chapter/event/volume domain entity names.
- **Outline-flavored conflict modal UX**: same structural flow as Strategy but with domain-appropriate copy ("This chapter changed…" / "This event changed…") and outline-flavored actions (Use current, Reapply my edit, Review side-by-side).
- **Non-spatial alternate views**: sortable chapter list + sortable timeline event list (with virtualization), satisfying the accessibility requirement for non-spatial navigation.
- **8 outline/timeline canvas-write DESIGN.md tokens**: `canvas-outline-volume-fill` + 4 chapter-card status tokens + `canvas-outline-timeline-event-pin` + `canvas-outline-foreshadow-edge` + `canvas-outline-timeline-marker` + `canvas-outline-conflict-marker` — filled with concrete light + dark values; token names preserved verbatim (V1.69 invariant continues).
- **Atomic outline markdown persistence**: outline body preserved under `RuntimeLockGuard`; never overwritten by canvas writes.
- **Body ownership invariant**: outline markdown body remains V1.65 editor-owned and is never overwritten by canvas writes.

### 18.2 The planning loop this enables

1. **Inspect and edit structure** — the author opens the Outline canvas, sees chapter nodes organized by volume, edits chapter properties (slug, word count, status) inline on the node or via the inspector.
2. **Timeline alongside outline** — the author positions timeline events relative to chapters, seeing both on the same canvas with timeline edges connecting to corresponding chapter nodes.
3. **Resolve conflicts** — when an orchestration session or another author changes the outline concurrently, the conflict modal surfaces the delta with chapter/event labels and recovery actions.

### 18.3 Non-goals for V1.72

- **No World KB canvas surface** — V1.73 candidate.
- **No in-canvas markdown body editing** — outline body remains V1.65 editor-owned; canvas writes only structure fields.
- **No canvas-pivot retirement of V1.65 outline editor** — deferred to V1.74+.

### 18.4 Wire contracts (V1.72)

**`wire_contracts_changed: TRUE`** (`@42ch/nexus-contracts` 0.7.0 → 0.8.0). Additive Outline+Timeline patch DTOs and routes through schemas/codegen.

---

## 19. Next stage — Canvas World KB β (V1.73)

V1.72 shipped the Outline+Timeline canvas. V1.73 completes the Canvas program's third surface — **World KB β** — the final of the three Draft canvas surfaces. The World KB canvas surfaces the author's accumulated world knowledge (characters, locations, items, events, organizations, conflicts, and computable derived entities) as a graph with entity nodes, promotion-state lifecycle badges, source-anchor provenance edges, and structured patch operations. This is the first canvas surface to require a **backend prerequisite**: promoting World KB read+write operations from CLI-direct-DB to first-class Daemon API routes with per-row OCC revision tracking.

> **Scope and roadmap SSOT**: [v1.73/delivery-compass.md](../iterations/v1.73/delivery-compass.md) §1.1 Tracks A+B, §1.3 wire contracts, §2 normative specs. This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 19.1 What ships in V1.73 (Track A — World KB β)

- **2 structured patch routes**: the UI calls two new Daemon API routes through `NexusClient`, following the V1.71/V1.72 patch-route OCC convention:
  - `POST /v1/local/world-kb/patch_entity` — entity title, body, aliases, and block_type edits on `kb_key_blocks`.
  - `POST /v1/local/world-kb/promote_candidate` — adopt/reject/merge promotion state machine on `kb_extract_jobs`.
- **Per-row OCC conflict detection** (LOCKED): reuses existing columns — `kb_key_blocks.revision` (from `20260525_kb_key_blocks.sql`) + `kb_extract_jobs.version` (from `202606190001_kb_extract_jobs_and_pool_version.sql`) — no new migration needed; no world-level revision counter. Stale writes return **409 `WorldKbConflictError`** with entity locator + current revision; validation failures return **422 `WorldKbValidationError`**.
- **Backend prerequisite — promoted World KB Daemon API routes**: World KB read+write operations promoted from CLI-direct-DB (`creator world kb adopt/reject/edit/delete`) to **4 first-class Daemon API routes** with OCC revision tracking, making the World KB a normative Daemon API surface (not CLI-only).
- **Canvas UI — World KB graph projection**: entity nodes (all block_type variants per entity-scope-model §5.1.1, plus computable blocks) + promotion-state lifecycle badges (pending → confirmed/rejected/merged, with `manual` state) + source-anchor provenance edges + computable badges (age, reference count, confidence). Route-split behind the shared Canvas Shell.
- **Canvas UI — entity inspector + promotion inspector**: entity inspector shows title, body, aliases, block_type, revision, and source anchors; promotion inspector shows candidate metadata (source, confidence, timestamp), current state, and promotion action buttons. Both inspectors surface inline validation errors and a save-in-progress indicator.
- **Canvas UI — KB-flavored conflict modal**: same structural pattern as Strategy (§17.1) and Outline (§18.1) conflict modals, but with World-KB-appropriate copy ("This entity changed while you were editing." / "This candidate's state changed while you were editing.") and KB-flavored actions (Use current, Reapply my edit, Review side-by-side). Cancel returns focus to the originating inspector.
- **Non-spatial alternate view**: sortable entity list (name, block_type, promotion state, last modified) with virtualization — satisfies the accessibility requirement for non-spatial navigation.
- **17 `canvas-worldkb-*` DESIGN.md tokens**: concrete light + dark values filled in `apps/web/DESIGN.md` + `apps/web/DESIGN.dark.md` for World KB node fills/borders/selection, promotion-state badges (confirmed/rejected/merged/pending), source-anchor edges, computable badges, conflict markers, and entity-inspector chrome — token names preserved verbatim (V1.69 invariant continues).
- **Track B companion — Outline β hardening**: 4 MEDIUM outline validation gaps closed (slug format, volume existence, foreshadow temporal order, published-chapter structural guard) + `outline-canvas.tsx` monolith split (≤250 lines per V1.71 Strategy pattern) + 2 release-hardening items (`tauri.conf.json` signing clarity + desktop release Rust cache coverage).

### 19.2 The knowledge loop this enables

1. **See the world as a graph** — the author opens the World KB canvas and sees all entities (characters, locations, items, etc.) laid out as a connected knowledge graph with source-anchor edges showing provenance — so the author understands the web of world knowledge the AI has accumulated and can trace every fact back to its source.
2. **Edit entity details** — the author selects an entity node, edits its title, body, aliases, or block_type via the inspector, and commits with OCC protection — so the author refines the AI-extracted world without overwriting concurrent extractions.
3. **Curate extracted knowledge** — the author opens the promotion inspector for a candidate fact, sees its source, confidence, and extraction context, then adopts, rejects, or merges it — so the author is the final curator of what goes into the canon.
4. **Resolve conflicts** — when the AI extracts a newer version of a fact while the author is editing, the KB-flavored conflict modal surfaces the delta and recovery actions — so the author never silently loses changes.

### 19.3 Non-goals for V1.73

- **No World KB relationships surface** — no `kb_relationships` table exists in the local DB; relationship semantics (directed, typed, confidence-weighted) require independent grill-me + architect lock. Deferred to V1.74: `tbd-v1.74-world-kb-relationships`.
- **No canvas-pivot retirement of V1.65 outline editor** — V1.74+ candidate.
- **No canvas-pivot retirement of KB CLI surface** — γ path rejected; KB CLI stays normative (V1.51).
- **9 hygiene items deferred** — virtualization/UI e2e/toast/atomic-rollback/useEffect/stale-docs/codegen-target/adapter-parity/can_edit_outline → V1.74 with durable plan-id pointer (`tbd-v1.74-hygiene`).

### 19.4 Wire contracts (V1.73)

**`wire_contracts_changed: TRUE`** (`@42ch/nexus-contracts` 0.8.0 → 0.9.0). Additive World KB DTOs and routes through schemas/codegen. New schemas under `schemas/local-api/world-kb/`.

---

## 20. Stage — Canvas World KB Relationships β (V1.74)

V1.74 completes the World KB canvas surface by promoting first-class typed relationships from the V1.73 deferred slot into a shipped authoring surface. The relationship route is reachable from both the canvas graph and the complete non-spatial relationship view; both entry points call the same Daemon API contract and preserve the §5 `NexusClient` boundary.

> **Scope and roadmap SSOT**: [v1.74/delivery-compass.md](../iterations/v1.74/delivery-compass.md) §0 grill decisions, §1.1 Track A, §1.3 wire contracts, and §2 normative specs. Architectural detail: [canvas-strategy-surface.md](canvas-strategy-surface.md) (V1.74 Shipped β), [entity-scope-model.md](entity-scope-model.md) §5.6, and [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md) §7.6.

### 20.1 What ships in V1.74 (Track A — relationship β)

- **Relationship patch route**: `POST /v1/local/worlds/{world_id}/kb/patch-relationship` (`WorldKbPatchRelationshipRequest` → `WorldKbPatchRelationshipResponse`) supports `add`, `update`, and `remove` with `expected_version` OCC on `kb_relationships.revision`. `GET /v1/local/worlds/{world_id}/kb/graph` populates `relationships[]`; symmetric rows emit derived reverse projections read-side without duplicate storage rows.
- **Canvas relationship-edge rendering**: graph edges display relationship type labels, confidence badges, and grounding badges. Symmetric reverse projections share the same storage `relationship_id` as the stored direction.
- **Relationship inspector**: create/edit/delete UI exposes source and target entity pickers, `WorldKbRelationshipKind` taxonomy dropdown, `custom_label` field for `custom`, `symmetric` toggle, optional anchor multi-picker, and display-only confidence. Inline validation surfaces self-loop, taxonomy, anchor, and confidence errors.
- **Three creation entry points**: drag from an entity handle, right-click an entity and choose “Connect to…”, or select **New Relationship** from the non-spatial relationship table. All entry points use the same quick-create flow and then the full relationship inspector.
- **Conflict modal reuse**: stale writes reuse the KB-flavored conflict modal pattern with relationship copy, current-vs-draft diff, and actions **Use current**, **Reapply my edit**, and **Cancel**.
- **Non-spatial relationship table**: the relationship table is a complete accessible write-surface with create/edit/delete parity, sortable columns, keyboard reachability, and accessible action labels.

### 20.2 The relationship loop this enables

1. **Connect entities** — the author creates a typed edge between two World KB entities from the canvas or table without leaving the UI.
2. **Qualify meaning** — the author chooses a core relationship kind or `custom_label`, marks symmetry when appropriate, and optionally attaches source anchors.
3. **Resolve conflicts** — concurrent daemon/canvas relationship edits return 409 before mutation; the conflict modal keeps the draft and lets the author refresh or reapply from the current version.

### 20.3 Non-goals for V1.74

- **No confidence weighting/filtering** — confidence is display-only in this stage.
- **No automatic relationship extraction** — relationships are author-driven; future extraction may suggest anchors or rows in a later iteration.
- **No canvas-pivot retirement of the V1.65 outline editor** — canvas-pivot is a V1.75+ candidate.
- **No relationship taxonomy management UI** — the core enum ships in contracts; authors use `custom` + `custom_label` for out-of-enum meanings.

### 20.4 Wire contracts (V1.74)

**`wire_contracts_changed: TRUE`** (`@42ch/nexus-contracts` 0.9.0 → 0.10.0). Additive World KB relationship DTOs and graph-response `relationships[]` item-schema refinement are generated from `schemas/local-api/canvas/world-kb/` and consumed by the local Web UI through `@42ch/nexus-contracts`.

---

*Local-first Web UI product contract. V1.64 Shipped (Control Room + Setup) → V1.65 §13 Content-Authoring → V1.66 §14 Desktop Shell → V1.67 §15 Surface Convergence & De-risk → V1.69 Design System Maturation & Canvas Draft → V1.70 §16 Canvas Strategy Implement (α) + CI/desktop-build optimization → V1.71 §17 Canvas Strategy Write-Boundary (β) → V1.72 §18 Canvas Outline+Timeline (β) → V1.73 §19 Canvas World KB (β) → V1.74 §20 Canvas World KB Relationships (β) → V1.77 §23 Findings-Remediation UI → V1.78 §24 Creator Memory Review-Loop UI. V1.75 roadmap: canvas-pivot candidate + 8 QC suggestions (`tbd-v1.75-qc-followup`). Design tokens: `apps/web/DESIGN.md` (V1.65 Standard+ + V1.66 desktop supplement + V1.69 Production migration + V1.70 canvas-token fill + V1.71 canvas-write tokens + V1.72 outline/timeline tokens + V1.73 canvas-worldkb tokens + V1.74 relationship tokens + V1.77 findings triage tokens + V1.78 creator-memory review-loop tokens).*

---

## 23. Next stage — Findings-Remediation UI (V1.77)

V1.76 shipped the World KB Relationship γ surface, completing the canvas program (V1.67–V1.76, 10 iterations). V1.77 pivots from the canvas to the **quality loop**: the Control-Room findings page — read-only since V1.64 — is promoted to a full **remediation authoring surface** that closes the "observe → triage → resolve" quality loop in the UI, exactly as the canvas closed the "steer → execute → review" writing loop. The backend already ships the full findings PATCH surface (6-state lifecycle adjacency enforcement, 7-field `UpdateFindingRequest` payload, full CRUD routes, stale-count endpoint); V1.77 consumes them from the web app with no new backend routes.

> **Scope and roadmap SSOT**: [v1.77/delivery-compass.md](../iterations/v1.77/delivery-compass.md) §0 grill decisions (Q1–Q4 locked), §1.1 Track A scope, §2 normative specs, §Phase 2b D4 (UX lock — authoritative), and §6 risk notes (all RESOLVED). This section records the product contract; the compass is authoritative for scope, batching, and residual tracking. Lifecycle detail: [findings-lifecycle.md](findings-lifecycle.md) (architect-drafted Master). API surface: [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md) (findings PATCH reference).

### 23.1 What ships in V1.77

The findings page gains three remediation affordances consuming the already-shipped `PATCH /v1/local/works/{work_id}/findings/{finding_id}` route (7-field `UpdateFindingRequest` payload, all fields optional), plus a detail-panel layout, TanStack Query mutations, and DESIGN.md triage tokens. No new backend routes — the PATCH surface already exists; V1.77 is frontend consumption only (D2 LOCKED: types are already generated and barrel-exported; no codegen config change needed).

- **Status transitions** — inline status dropdown or action buttons driving the 6-state findings lifecycle: `open` → `triaged` → `in_review` → (`resolved` | `wont_fix` | `duplicate`). Invalid transitions are disabled client-side per the server-enforced adjacency (DAO `is_valid_transition()` table; illegal transitions return HTTP 422 `INVALID_TRANSITION`). Status change persists via PATCH, the findings list refetches, and the stale-findings count in the daemon status bar updates on mutation invalidation.

- **`target_executor` assignment** — dropdown/selector routing the finding to `brainstorm`, `write`, `master`, or `none`. `target_executor` is an *assignment* (route hint for triage), not an auto-trigger — re-running an orchestration session stays a deliberate canvas/CLI action (grill Q2 option C rejected). Valid values per `crates/nexus-local-db/src/findings.rs:192`.

- **Inline edit** — finding detail/inspector panel exposes the 7 `UpdateFindingRequest` fields (all optional): `title`, `description`, `severity`, `kind`, `rule_suggestion`, `status`, `target_executor`. Edits persist via PATCH with optimistic TanStack Query mutations (`useUpdateFinding`); the findings list + stale-count queries refetch on mutation invalidation. The inspector shows full finding context: `chapter`, `source_schedule_id`, `routing_hint`, `kind`, timestamps.

- **Detail-panel + row-action hybrid layout** (D4 LOCKED) — the findings page remains a Control-Room table (not a canvas graph); a detail/inspector panel with the three remediation affordances supplements row-level status/severity badges, reusing existing `Table` + `StatusBadge` components. Row-level actions (status dropdown, assignment selector) enable quick triage without opening the inspector. A11y: the canvas non-spatial-alternate-view discipline applies to canvas graph surfaces, not to Control-Room tables.

- **TanStack Query mutations with optimistic updates** — `useUpdateFinding` updates locally before the server responds, rolls back on error, and invalidates the findings list + stale-count queries on success. In the last-writer-wins model (D1b — no OCC, no revision column, no conflict modal; the quality loop is single-author-triage), optimistic updates are safe with no conflict modal needed.

- **DESIGN.md tokens** — findings status badges (6 states: `open`, `triaged`, `in_review`, `resolved`, `wont_fix`, `duplicate`), severity colors (reusing existing `severityVariant` where possible), triage chrome (action buttons, assignment selector, inline-edit affordance). Token names preserved verbatim (V1.69 invariant continues).

- **Codegen wiring** — `UpdateFindingRequest` and `CreateFindingRequest` are already generated and barrel-exported by `@42ch/nexus-contracts` (D2 LOCKED: codegen is glob-based, both types exist on disk at `packages/nexus-contracts/src/generated/local-api/findings/` and `crates/nexus-contracts/src/generated/local_api/findings/`). V1.77 adds imports in `apps/web/src/lib/nexus/types.ts` + wires `getFinding`/`updateFinding` onto the `NexusClient` interface + `BrowserClient`/`TauriClient` implementations. No codegen config change, no schema change.

### 23.2 The triage loop this enables

The UI closes the **observe → triage → resolve** quality loop for an author who previously had to drop to the CLI:

- **Triage status** — *As an author*, I can review a finding, assess its severity and context, and advance its status through the 6-state lifecycle (`open` → `triaged` → `in_review` → `resolved` / `wont_fix` / `duplicate`) directly from the findings page, so the quality loop progresses without CLI commands.

- **Route by assignment** — *As an author*, I can assign a finding's `target_executor` to `brainstorm`, `write`, `master`, or `none` — routing it to the appropriate stage of the writing loop — so my triage decisions are visible and actionable when I return to the canvas or CLI to re-run orchestration.

- **Edit finding details inline** — *As an author*, I can correct a finding's title, description, severity, kind, or rule_suggestion in the inspector panel without dropping to the terminal, so I can refine quality metadata as I triage.

- **Cross-profile triage** — *As an author*, I can triage findings for any Work (novel, essay, game-bible, or script) from the same findings page — the DB and Daemon API are already profile-agnostic, so no profile-specific UI restriction is introduced.

### 23.3 Non-goals for V1.77

Explicitly deferred with rationale (compass §1.2 + grill Q2 option C rejected; satisfies the Durable Roadmap Gate):

- **One-click orchestration re-trigger from a finding** — **rejected** (grill Q2, option C). Re-running a brainstorm/write session stays a deliberate canvas/CLI action. The canvas is the intended steering surface for re-runs; a finding "Re-run" button would couple UI remediation to scheduler semantics and overlap the canvas steering surface. `target_executor` is an *assignment* (route hint), not an auto-trigger.

- **Findings producer changes** — V1.77 consumes findings the quality loop already produces. Changing *what* findings are produced (new rules, new review-master output, new extraction) is out of scope.

- **New quality-loop rules / rubrics** — the 五问 rubric and review-master presets are unchanged.

- **New canvas surfaces** — the canvas program is complete (V1.67–V1.76). V1.77 deepens the findings *Control-Room* surface, not the canvas.

- **Body editor** — **rejected** direction (Nexus is AI-autonomous executor; AI owns prose).

- **Platform publish** — platform paused (local-only).

- **Mobile** — future scope.

### 23.4 Wire contracts (V1.77)

**`wire_contracts_changed: FALSE`** (LOCKED by Phase 2b architect). The findings schemas (`update-finding-request`, `create-finding-request`, `finding-detail-response`, `list-findings-query`/`response`, `stale-findings-response`) already exist on disk under `schemas/local-api/findings/` and are already fully codegen'd into both TypeScript (`packages/nexus-contracts/src/generated/local-api/findings/`) and Rust (`crates/nexus-contracts/src/generated/local_api/findings/`). All generated types are already barrel-exported by `@42ch/nexus-contracts` (version `0.12.0` stays). V1.77 only adds consumer-side imports of already-available types — a purely additive frontend change that does not constitute a wire-contracts bump. The web app's `NexusClient` interface gains `getFinding` and `updateFinding` methods consuming the existing `UpdateFindingRequest` and `FindingDetailResponse` DTOs.

---

## 24. Next stage — Creator Memory Review-Loop UI (V1.78)

V1.77 closed the quality loop in the UI. V1.78 closes the **creator self-loop** (capture → review → internalize): the Control-Room gains a creator-scoped **Memory** page that lets an author review the pending captures their sessions produced, summarize them into long-term memory, and browse the resulting fragments — all from the web app, without dropping to the terminal. The memory Daemon API has shipped since V1.33 (`handlers/memory.rs`) but was never contracted; V1.78 publishes the OSS schemas (`schemas/local-api/memory/`), normalizes the hand-written handler DTOs to generated types (fixing the daemon-runtime no-hand-written-DTO invariant), and consumes the typed surface from the web app. `createPendingReview` stays CLI/producer-only — the session-end capture pipeline owns creation; the UI is review/consume-only, exactly mirroring V1.77's `createFinding` CLI-only decision.

> **Scope and roadmap SSOT**: [v1.78/delivery-compass.md](../iterations/v1.78/delivery-compass.md) §0 grill decisions, §1.1 Track A scope, §Phase 2b D-UX (UX lock — authoritative), and §Phase 2b open items #1–#5 (frontend wiring). This section records the product contract; the compass is authoritative for scope, batching, and residual tracking. Batch 1 (contracts/backend) ships the schemas + codegen + handler DTO normalization; Batch 2 (this frontend stage) consumes the typed surface.

### 24.1 What ships in V1.78 (frontend stage)

The Memory page (`apps/web/src/pages/memory-page.tsx`, NEW) gains three affordances consuming the shipped memory Daemon API, all creator-scoped (`creator_id` on every endpoint; the daemon rejects a mismatched creator with 403). No new backend routes — Batch 1 contracts the existing surface; Batch 2 consumes it.

- **Pending-review list with count badge + delete** — a Control-Room table (cursor-paginated, default 50, max 250) driven by `GET /v1/local/memory/pending-review?creator_id={id}`. The header shows a live count badge from `GET …/pending-review/count?creator_id={id}` (`CountPendingReviewsResponse { count }`, polled). Each row shows a humanized `task_kind` chip, truncated `session_id`, truncated `raw_digest`, and `created_at` in local time. A per-row delete action calls `DELETE …/pending-review/{pending_id}?creator_id={id}` with a web-friendly confirmation; the row disappears optimistically and the count decrements.

- **"Review & Summarize" action** — a prominent primary CTA (enabled only when `count > 0`) triggering `POST /v1/local/memory/review` (`ReviewRequest { creator_id }`). The UI shows a processing state during the server-side pipeline (the passthrough classifier promotes/fragments/drops each pending row), then surfaces `ReviewResponse { promoted, fragmented, dropped }` in a confirmation toast (e.g., "Review complete — 3 promoted to long-term memory, 5 saved as fragments, 2 dropped"). On success, the pending-review list, count, and fragments queries all refresh.

- **Fragments browser** — a read-only list from `GET /v1/local/memory/fragments?creator_id={id}&keyword={opt}&limit={n}` (default 50, optional case-insensitive keyword filter). Each row shows `fragment_id` (monospace) and `summary`. Fragments are produced **only** by the `review` route — no manual CRUD exists on this surface.

- **Inspector panel** — selecting a pending-review row opens a side inspector (matching the V1.77 `FindingDetailPanel` layout) showing all 6 `PendingReviewInfo` fields: `pending_id` (monospace badge), `session_id`, `world_id` (or "(none)" if absent — open item #3), `task_kind` (humanized chip), `raw_digest` (scrollable preformatted area), `created_at` (RFC 3339 → author's local time). Clicking the selected row again or the panel's delete affordance dismisses it.

- **`NexusClient` promotion** — five methods added to the interface (`listPendingReviews`, `countPendingReviews`, `deletePendingReview`, `reviewMemory`, `listMemoryFragments`) and implemented on `BrowserClient` (inherited unchanged by `TauriClient`). Adapter-contract parity guard extended. `createPendingReview` is intentionally absent (CLI/producer-only).

- **TanStack Query mutations with optimistic updates** — `useDeletePendingReview` removes the row and decrements the count before the server responds, rolls back on error, and invalidates pending-list + count + fragments on settle. `useReviewMemory` surfaces the result counters in a toast and invalidates the same three query sets on success. The pending-review list uses `useInfiniteQuery` for cursor pagination (reuses the `LoadMore` component).

- **`creator_id` UI source** (open item #1) — the active creator id is derived from the most recent session/schedule (`useActiveCreatorId`), mirroring the canvas's `useDerivedCreatorId` derivation (`apps/web/src/lib/canvas/use-strategy-data.ts`). The daemon model is single-active-creator (config.toml); sessions/schedules are themselves creator-scoped, so their `creator_id` is the active creator. A first-class active-creator endpoint/context is a future surface.

- **DESIGN.md tokens** — 13 memory tokens (`memory-pending-count`, `memory-review-button`, `memory-fragment-summary`, `memory-fragment-id`, 5 `memory-task-kind-*` chips, `memory-inspector-header`/`-field-label`/`-field-value`, `memory-fragment-filter-input`). Token names preserved verbatim (V1.69 invariant continues).

### 24.2 The review loop this enables

The UI closes the **capture → review → internalize** self-loop for an author who previously had no visibility into the memory pipeline:

- **Review pending captures** — *As an author*, I can see every pending capture my sessions produced (with a live count badge), inspect its full context (session, world, task kind, raw digest, timestamp), and delete a capture I do not want internalized — so the memory pipeline does not silently accumulate noise.

- **Summarize into long-term memory** — *As an author*, I can trigger Review & Summarize to run the rule-based classifier over my pending queue, and see exactly how many captures were promoted to long-term memory, saved as fragments, or dropped — so I understand what my memory actually contains.

- **Browse long-term fragments** — *As an author*, I can browse my long-term memory fragments (optionally filtered by keyword) and read their summaries — so I can recall what the system has internalized about my creative work without dropping to the CLI.

### 24.3 Non-goals for V1.78

Explicitly deferred with rationale (compass §1.2 + D-UX LOCKED; satisfies the Durable Roadmap Gate):

- **`createPendingReview` from the UI** — **rejected** (D-UX LOCKED). The session-end capture pipeline owns pending-review creation; the UI is review/consume-only. Mirrors V1.77's `createFinding` CLI-only decision.

- **Fragment CRUD** — **rejected** (D-UX LOCKED). Fragments are produced only by the `review` route (the sole fragment producer). No manual fragment create/edit/delete exists in the backend and none is introduced.

- **LLM-backed summarizer** — **out of scope**. The `review` route runs the shipped passthrough summarizer (`UNTRUSTED` provenance, 256 KiB truncation); V1.78 documents shipped behavior and must not introduce a new LLM-backed summarizer or change review rules.

- **Active-creator context provider** — the current derivation (sessions/schedules) is sufficient for the single-active-creator model. A first-class active-creator endpoint/context is a future surface.

- **New canvas surfaces** — the canvas program is complete (V1.67–V1.76). V1.78 deepens the Control-Room surface, not the canvas.

- **Mobile** — future scope.

### 24.4 Wire contracts (V1.78)

**`wire_contracts_changed: TRUE`** (additive — LOCKED by Phase 2b architect). V1.78 publishes the memory OSS schemas that the runtime already serves but never contracted. The new files live under `schemas/local-api/memory/`; no existing schema is modified, and handler behavior is unchanged (the schemas mirror the existing hand-written runtime DTOs field-for-field). Codegen auto-discovers the net-new files via the existing glob. `@42ch/nexus-contracts` additive bump **0.12.0 → 0.13.0**; new memory types are barrel-exported and no existing type changes. The web app's `NexusClient` interface gains five methods consuming the generated memory DTOs (`ListPendingReviewsQuery`/`Response`, `CountPendingReviewsResponse`, `DeletePendingReviewResponse`, `ReviewRequest`/`Response`, `ListMemoryFragmentsQuery`/`Response`, `PendingReviewInfo`, `MemoryFragmentInfo`).

---

## 25. Next stage — Author Reflection: Reading Surface + SOUL Visualization (V1.79)

V1.78 closed the third and final author-in-command loop (creator memory). V1.79 is the first **post-loop-closure iteration** and takes the natural next step: rather than opening a new loop, it deepens the author's ability to **reflect on** what the closed loops produce. Two independent UI tracks ship under the shared theme **"Author Reflection"** — a manuscript reading surface with in-context maturation indicators (Track A) and a SOUL personality visualization over internalized memory fragments (Track B).

> **Scope and roadmap SSOT**: [v1.79/delivery-compass.md](../iterations/v1.79/delivery-compass.md) §1 grill decisions, §2 scope, and §6 acceptance criteria. This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 25.1 What ships in V1.79 (dual-track)

**Track A — Manuscript reading surface (P0)**

- **Designed reading experience**: the post-V1.75-pivot residual `chapter-page.tsx` (bare read-only body render + frontmatter strip + Copy Path + "Edit outline → Canvas" redirect) is promoted to a designed reading surface with legible reading typography (light + dark), chapter/volume navigation (prev/next, keyboard ←/→), and session-only reading progress. No new write routes — body-ownership invariant preserved (canvas remains the sole authoring surface).
- **In-context lightweight maturation indicators**: three indicators visible on the reading surface without navigation — chapter completion-state badge (from `work_chapters.status`), World KB density count (from `kb_key_blocks` count), and open-findings count (from `findings` non-terminal lifecycle rows). Read-only consumption of existing data.
- **Profile scope**: novel-first; other profiles (essay/game-bible/script) render read-only-prose-compatible via the same body render. Profile-specific reading chrome deferred. `wire_contracts_changed: FALSE` for Track A.

**Track B — SOUL personality visualization (P1)**

- **Keyword clusters**: frequency/cluster visualization of the creator's internalized memory fragment `keywords`, scoped per creator (`creator_id` from the `n` table). Surfaces the top accumulated themes — what the author and their AI assistants have been focusing on.
- **Temporal drift timeline**: `created_at`-axis showing fragment accumulation + keyword composition shift over time, with fragment count folded into the timeline. Answers "how has my creative focus shifted?" — the core reflection insight.
- **Sparse-data graceful degradation**: three states (empty → low-data → rich) with empathetic, encouraging copy at each state. New creators see a forward-looking empty state, not a broken chart.
- **Memory page integration**: both viz surfaces integrate into the V1.78 Control-Room Memory page as a new tab/section — no new top-level route.
- **Wire additive**: `memory-fragment-info` gains optional `keywords` (array) + `created_at` (string, RFC 3339). `@42ch/nexus-contracts` 0.13.0 → 0.14.0 (additive only; handler behavior unchanged — DAO already stores these fields).

**B companion**: light — close `R-V178P0-QC3-001` (web typecheck build-order CI/prebuild wrapper). `R-V178P0-QC3-003` (synchronous-review reliability) recorded in reliability roadmap, future iteration.

### 25.2 The reflection loops this enables

**Track A — review-augmented reading**:

1. **Read as a reader** — the author opens any chapter in a comfortable, distraction-free reading view with book-like typography, rather than raw markdown.
2. **See maturity at a glance** — without leaving the reading surface, the author sees the chapter's completion state, World KB richness, and open findings count — making the reading surface actionable for review.
3. **Navigate the manuscript** — chapter/volume navigation with session progress lets the author flow through their manuscript naturally.

**Track B — seeing who you are becoming**:

1. **Discover creative themes** — the author sees keyword clusters surfacing what their creative work has internalized into SOUL memory.
2. **Track focus over time** — the temporal drift timeline shows how the author's themes have shifted — "am I drifting toward or away from the writer I want to be?"
3. **Start from nothing with confidence** — new creators see an encouraging empty state that explains the feature's value proposition and what actions will populate it.

### 25.3 Non-goals for V1.79

- **Standalone maturation dashboard** (multi-chart cross-Work/World aggregate; BL-09) — deferred. **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-30 (standalone maturation dashboard).
- **Independent growth-curve view** as a separate SOUL visualization — folded into temporal drift; standalone deferred.
- **Persisted reading progress + annotations/highlights (MVP)** — **Shipped in V1.89**. See §28 (V1.89 Amendment — Deeper Manuscript Reading). Session-only behavior was V1.79; persistence + highlights (with drift notice and body-ownership invariant) shipped in V1.89.
- **Per-World SOUL filtering** — deferred. **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-32 (per-World SOUL filtering).
- **Profile-specific reading chrome** (essay section breaks, game-bible cross-refs, novel typography presets) — deferred (BL-11 tracker row). **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-31 (profile-specific reading chrome; BL-11).
- **DF-49 (Standalone MCP server)** — **cancelled** (not deferred), conflicts with ACP-client product direction and creates circular-invocation risk.
- **Any new write route on the reading surface** — read-only consumption only.
- **New canvas surfaces** — the canvas program is complete (V1.67–V1.76). V1.79 deepens the Control-Room reading + memory surfaces.

### 25.4 Wire contracts (V1.79)

**`wire_contracts_changed: TRUE`** (additive — Track B only; Track A is read-only consumption of existing data). `memory-fragment-info` gains optional `keywords` (array of string) + `created_at` (string, RFC 3339) in `schemas/local-api/memory/memory-fragment-info.schema.json`. `@42ch/nexus-contracts` **0.13.0 → 0.14.0** (additive — no existing type changes; handler behavior unchanged). Track A changes no schemas.

---

## 26. Next stage — Creator SOUL Maturation: Narrative, Projection, Growth, Refresh (V1.81)

V1.79 gave the author their first reflection surface — keyword clusters and a temporal drift timeline over internalized SOUL fragments. V1.80 stabilized the review pipeline that feeds those fragments. V1.81 deepens the reflection axis with four additions under the product theme **"Creator SOUL Maturation"**, anchored on a definitive product model:

> **Creator SOUL is the creator's core creative identity — world-agnostic. It is the *whole*: all accumulated `memory_fragments`. A per-World SOUL projection is the Creator SOUL's inclination within a specific world — a *subset* of fragments filtered by the world they emerged from. It is a drill-down view, not a separate identity. The LLM personality narrative synthesizes the whole into a reflective "who you are becoming" statement and is world-agnostic by definition; the world projection only filters the read-side viz (keyword clusters, temporal drift, growth-curve).**

This product model is user-visible in the UI — the world selector explicitly frames a world projection as "a subset of your Creator SOUL," not a separate identity.

> **Scope and roadmap SSOT**: [v1.81/delivery-compass.md](../iterations/v1.81/delivery-compass.md) §1 grill decisions, §2 scope, and §6 acceptance criteria. This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 26.1 What ships in V1.81 (four spec points)

**SP-1 — Creator-SOUL Narrative (headline, ④)**

- **Narrative card** (`soul-narrative-card.tsx`): a new surface above the existing keyword/drift viz on the SOUL tab. The card synthesizes an LLM-generated reflective narrative — "who you are becoming" as a creative identity — from the author's accumulated fragment themes, temporal shifts, and preoccupations. The narrative is generated on-demand ("Reflect on my SOUL") and persisted (cached); it is not regenerated on every page load.
- **Five UX states (each testable)**:
  1. `ungenerated` — CTA card with a "Reflect on my SOUL" button; preview hint text below explaining what the narrative will show.
  2. `generating` — loading skeleton with pulse; button shows spinner + "Reflecting…"; no timeout panic.
  3. `current` — prose block with `generated_at` timestamp; "Re-reflect" secondary action.
  4. `stale` — cached narrative still visible; banner above: "You've grown since this reflection — new fragments have arrived." with "Re-reflect" CTA on the banner.
  5. `insufficient-data` — empty-state illustration: "Your SOUL is still forming. Keep writing and reviewing — once you've accumulated enough creative experience, Nexus can reflect on who you are becoming." Below, a fragment count with "X more to go" when close to the threshold.
- **Narrative quality threshold** (product-gated, architect-enforced in the prompt contract): a narrative is "good enough" when it (1) references at least two distinct theme keywords from the creator's fragment clusters, (2) references at least one shift or development over time, and (3) ends with a forward-looking reflection or question. Below the minimum-fragment threshold, the endpoint returns the insufficient-data state rather than risking a thin/generic narrative.
- **Consumes**: `POST /v1/local/memory/soul/reflect` (new endpoint, P0) via a new `useSoulNarrative` query/mutation. The request carries required `creator_id` and optional `force_regenerate`; the response exposes `state`, cached/generated narrative fields when present, stale snapshots, current counts, and insufficient-data thresholds.

**SP-2 — World Projection Selector (①)**

- **World selector**: a dropdown/list control in the SOUL section header, defaulting to "All worlds" (the whole Creator SOUL). Selecting a specific world re-scopes the keyword clusters, temporal drift, and growth-curve to that world's fragment subset. It does **not** re-scope the LLM narrative or its generation metadata in V1.81; the narrative remains Creator-level / world-agnostic.
- **Subset semantics (UX contract)**:
  - Worlds with fragments list their name + fragment count (e.g., "Eryndor (42 fragments)"); worlds with Works but no fragments may appear with the subset-empty state.
  - Worlds with zero fragments and zero Works are **omitted** from the selector — no dead-end empty options.
  - "All worlds" is always the default; the label clarifies "your whole Creator SOUL."
  - If a selected Work-backed world has no fragments yet, the viz area shows the subset-empty copy: "No fragments in this world yet — your Creator SOUL is still shaped by your work here when fragments arrive."
- Drives a `world_id` query param on the fragments query; coordinates with the page-level `fragmentKeyword` lift pattern in `memory-page.tsx`.

**SP-3 — Growth-Curve (②, BL-10)**

- **Growth-curve component** (`growth-curve.tsx`): cumulative fragment count over time as a simple line/area chart, independent of the temporal-drift timeline (which answers "how has my focus shifted?" — the growth-curve answers "how much have I accumulated?").
- **Three density states** (reuses the V1.79 `densityFor` branching):
  - `empty` (0 fragments): forward-looking illustration + "Your SOUL begins here — every review session adds a fragment to your creative growth."
  - `low-data` (1–9 fragments): simple chart + "Your SOUL is taking shape. Keep writing to see your growth curve emerge."
  - `rich` (≥10 fragments): full cumulative curve with axis labels and a summary stat.
- Respects the world projection (re-scopes when a world is selected).

**SP-4 — Auto-Refresh (③)**

- Poll interval on the SOUL fragments query (react-query `refetchInterval`) + invalidation of all SOUL queries when the review mutation settles.
- After a review session, the SOUL viz refreshes without a manual reload — the author sees the new fragments immediately.

### 26.2 The reflection loops this deepens

**SP-1 (Narrative) — seeing who you are becoming in prose**:

1. **Reflect on demand** — the author triggers a synthesis and reads a narrative statement of their accumulated creative identity, not a keyword dump.
2. **Stay current** — the stale banner prompts re-reflection after growth, making the narrative a living mirror rather than a frozen snapshot.
3. **Start with confidence** — new creators see an encouraging empty state that explains the feature's value, not a thin/generic LLM output.

**SP-2 (Projection) — understanding your creative self across worlds**:

1. **See the whole** — default "All worlds" shows the author's complete creative identity.
2. **Drill into a world** — selecting a world shows how the author's themes manifest within that world's context, as a subset.
3. **Honest subset rendering** — the UI never pretends a projection is the whole; empty worlds are omitted; subset-empty states are labeled clearly.

**SP-3 (Growth) — watching accumulation**:

1. **Growth at a glance** — the curve answers "am I accumulating creative experience?" separately from "is my thematic focus shifting?"
2. **Degrade gracefully** — new creators see a forward-looking state, not a broken chart.

**SP-4 (Refresh) — responsiveness**:

1. **No manual steps** — finishing a review session flows naturally into seeing the updated SOUL.

### 26.3 Non-goals for V1.81

- **Per-World LLM narratives** — deferred. **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-32 (per-World SOUL narratives + filtering).
- **Narrative editing / curation by the author** — deferred. **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-33 (SOUL narrative curation/editing + export/share).
- **Narrative export / share** — deferred. **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-33 (SOUL narrative curation/editing + export/share).
- **Async background-job infrastructure** — on-demand generation only (consistent with V1.80 discipline).
- **BL-09 standalone maturation dashboard** — remains backlog.
- **BL-11 deeper manuscript reading (MVP slice)** — **Shipped V1.89**: persisted reading progress + character-offset annotations/highlights with drift notice. Profile-specific reading chrome remains deferred (see tracker).
- **Realtime websocket / push** — poll + invalidation only.
- **Rewrite of existing keyword/drift viz** — the world filter is surgical; no restructuring beyond adding the selector.

### 26.4 Wire contracts (V1.81)

**`wire_contracts_changed: TRUE`** (additive — P-1 creates schemas, P0 runs codegen). Two additive changes:

1. `memory-fragment-info` gains optional `world_id` (`string | null`; consumers treat absent/null as Creator-core-only). `list-memory-fragments-query` gains optional `world_id` query param (omitted = whole Creator SOUL; present = specific world subset).
2. New soul-narrative schemas: `soul-narrative-request.schema.json` (required `creator_id`, optional `force_regenerate`) + `soul-narrative-response.schema.json` (`state`, optional narrative/cache fields, stale flag, current counts, and threshold fields for the insufficient-data state).

`@42ch/nexus-contracts` **0.15.0 → 0.16.0** (additive — no existing type changes). The web app consumes the new types as frozen contracts from P-1; P0 owns the codegen commit.

---

## 27. Next stage — SOUL Completion: per-World Narrative + Titled Worlds Selector (V1.82)

V1.81 shipped the Creator-SOUL narrative (the whole, world-agnostic) and the world projection that filters read-side visualization only. V1.82 completes the SOUL surface: each world now gets its own narrative — the Creator SOUL's inclination within that world — and the selector is wired to real world titles from the existing endpoint.

> **Creator SOUL is the creator's core creative identity — world-agnostic. It is the *whole*. A per-World narrative is the Creator SOUL's inclination within a specific world — a *subset* of that whole. Each has its own narrative. "All worlds" shows the Creator-level narrative (V1.81 behavior unchanged). Selecting a world shows that world's own per-World narrative.**

This model is locked in the UI. The world selector shows titles (not ids) and drives both visualization scope and narrative scope. A world may have insufficient data for its own narrative even when the Creator whole does not.

> **Scope and roadmap SSOT**: [v1.82/delivery-compass.md](../iterations/v1.82/delivery-compass.md) §1 grill decisions, §2 scope, §6 acceptance criteria, and §7 non-goals. This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 27.1 What ships in V1.82 (two spec points)

**SP-1 — per-World Narrative (headline)**

- The narrative card re-renders when the world selector changes.
  - "All worlds" → the Creator-level narrative (V1.81 behavior and states unchanged).
  - A specific world → that world's per-World narrative, generated on-demand and invalidated per (creator, world).
- Per-world narrative states are independent of the Creator-level narrative. A world may show `insufficient_data`, `ungenerated`, or `stale` even when the Creator-level narrative is `current`. The card surfaces the world-specific state; it does not fall back to or restate the Creator-level narrative.
- **Distinct + valuable gate** (product threshold): a per-World narrative is shown only when it (a) surfaces at least one theme or temporal shift that is specific to that world's fragments and not prominent in the Creator-level narrative, or (b) meets the same three-point quality bar (≥2 theme keywords + ≥1 shift + forward-looking close) scoped to the world's own fragments. A thin restatement of the Creator-level narrative that adds no world signal fails the gate and surfaces the per-world insufficient-data state instead.
- The per-World narrative uses the same five UX states as V1.81 (ungenerated / generating / current / stale / insufficient-data), with world-specific copy and thresholds.
- Selector-driven: "Reflect on this world's SOUL" (or equivalent clear label) triggers generation for the selected world.

**SP-2 — Titled world selector (worlds-endpoint wiring)**

- The world selector consumes the existing `GET /v1/local/narrative/worlds` (already returns `title`; workspace-scoped under the single-creator local model). No new backend endpoint. Ownership of a selected world is enforced server-side at `/soul/reflect` via `narrative_worlds.owner_creator_id` — the selector itself does not perform ownership filtering.
- Renders world **titles** (not raw ids). "All worlds" remains the default and explicitly labels "your whole Creator SOUL."
- Includes Work-backed worlds (those with Works but no fragments yet). These appear with an honest subset-empty state: "No fragments in this world yet — your Creator SOUL is still shaped by your work here when fragments arrive."
- Worlds with zero fragments and zero Works remain omitted (no dead-end options).
- Selector change is the single source of truth: it re-scopes the keyword clusters, temporal drift, growth-curve, *and* the narrative card. No stale narrative from a prior selection remains visible after a change.

### 27.2 The reflection loops this completes

**The Creator SOUL is the whole; a per-World narrative is a subset — each has its own narrative**:

1. **See the whole** — default "All worlds" shows the complete Creator creative identity (V1.81 narrative).
2. **Drill into a world** — selecting a world shows how the author's themes manifest within that world's context as a distinct per-World narrative, not just filtered viz.
3. **Honest per-world states** — a world with little data shows its own insufficient state; the UI never pretends a thin per-World narrative is the Creator-level narrative.

1. **See the whole** — default "All worlds" shows the complete Creator creative identity (V1.81 narrative).
2. **Drill into a world** — selecting a world shows how the author's themes manifest within that world's context as a distinct narrative, not just filtered viz.
3. **Honest per-world states** — a world with little data shows its own insufficient state; the UI never pretends a thin world narrative is the Creator whole.
4. **Titled, reachable worlds** — authors choose by name; Work-backed worlds are visible even before fragments arrive.

**Distinct vs thin product gate** (enforced at the surface):

- Worlds that have accumulated enough specific signal show a valuable per-World narrative.
- Worlds that have not yet produced world-specific signal show the graceful insufficient state rather than a thin restatement of the Creator-level narrative.

### 27.3 Non-goals for V1.82 (see also compass §7)

- Narrative comparison across worlds (side-by-side or diff view) — out of scope.
- Narrative editing or curation by the author — read-only LLM output; only "re-reflect." **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-33 (SOUL narrative curation/editing + export/share).
- Narrative export or share paths. **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-33 (SOUL narrative curation/editing + export/share).
- Realtime websocket / push (poll + invalidation only).
- A new backend worlds endpoint (existing one is wired).
- Rewrite of V1.81 SOUL components beyond the per-World extension (surgical).

### 27.4 Wire contracts (V1.82)

**`wire_contracts_changed: TRUE`** (additive). One additive change:

- `soul-narrative-request` gains optional `world_id` (absent / null = Creator-level narrative; present = per-World narrative). The `soul-narrative-response` shape is unchanged (reuses the V1.81 state enum and fields, now scoped per world).

`@42ch/nexus-contracts` **0.16.0 → 0.17.0** (additive). The web app consumes the frozen contract types from P-1; the worlds list endpoint is pre-existing and already sufficient for titles.

---

*Local-first Web UI product contract. V1.64 Shipped (Control Room + Setup) → V1.65 §13 Content-Authoring → V1.66 §14 Desktop Shell → V1.67 §15 Surface Convergence & De-risk → V1.69 Design System Maturation & Canvas Draft → V1.70 §16 Canvas Strategy Implement (α) + CI/desktop-build optimization → V1.71 §17 Canvas Strategy Write-Boundary (β) → V1.72 §18 Canvas Outline+Timeline (β) → V1.73 §19 Canvas World KB (β) → V1.74 §20 Canvas World KB Relationships (β) → V1.77 §23 Findings-Remediation UI → V1.78 §24 Creator Memory Review-Loop UI → V1.79 §25 Author Reflection: Reading Surface + SOUL Visualization → V1.81 §26 Creator SOUL Maturation: Narrative, Projection, Growth, Refresh → V1.82 §27 SOUL Completion: per-World Narrative + Titled Worlds Selector → **V1.89 §28 Deeper Manuscript Reading (BL-11 MVP: persisted progress + annotations/highlights)**. Design tokens: `apps/web/DESIGN.md` (V1.65 Standard+ + V1.66 desktop supplement + V1.69 Production migration + V1.70 canvas-token fill + V1.71 canvas-write tokens + V1.72 outline/timeline tokens + V1.73 canvas-worldkb tokens + V1.74 relationship tokens + V1.77 findings triage tokens + V1.78 creator-memory review-loop tokens + V1.79 reading-surface tokens + SOUL-viz tokens + V1.81 narrative-card tokens + growth-curve tokens + V1.82 per-world narrative + titled-selector tokens + V1.89 reading-annotation-highlight tokens × 4 + annotation-inspector tokens + selection-toolbar tokens).*

---

## 28. V1.89 Amendment — Deeper Manuscript Reading

**Status**: In progress (V1.89) — **MVP slice**: persisted reading progress + character-offset annotations/highlights with drift notice. Profile-specific reading chrome deferred.

### 28.1 Author-visible outcomes

- Reopening a chapter restores the author's last scroll position automatically (across reloads, tabs, and the Tauri desktop shell).
- Selecting text in the reading surface creates a persistent highlight with an optional note. The highlight survives navigation away from the chapter and reappears when the author returns.

### 28.2 Scope

Persisted reading progress is tracked per (creator, work, chapter).

Annotations and highlights are anchored by character offsets into the current body plain text of the chapter. The color enum is limited to `{yellow, blue, green, pink}`. Each highlight may carry an optional free-text note.

### 28.3 Read-only body invariant

The reading surface does not mutate `body_path` or outline files. Canvas remains the sole authoring surface per the V1.75 pivot. Any future "accept annotation into manuscript" flow must go through the canvas / Outline API, not the reading surface.

### 28.4 Drift notice

When a highlight's stored offsets no longer fit the current body text (after a body edit performed in the canvas), the UI must show a clear, non-blocking notice instead of mis-rendering the highlight or silently failing.

### 28.5 Non-goals

- Profile-specific reading chrome (essay section breaks, game-bible cross-reference overlays, novel typography presets, etc.).
  > **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-31 (profile-specific reading chrome; BL-11).
- Standalone maturation dashboard (BL-09).
  > **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-30 (standalone maturation dashboard; BL-09).
- Body or outline editing from the reading surface.
- Annotation range reconciliation or fingerprinting across body edits.
- Rich-text or threaded annotations on highlights.
- Real-time sync or cloud features.

---

## 29. Information Architecture (V1.94)

**Status**: Draft (V1.94) — normative contract frozen by P-1; implement authority P1.
**Iteration compass**: [v1.94/delivery-compass.md](../iterations/v1.94/delivery-compass.md) §1 (locked decisions D1, E1, F1, C1, G1) + §5 (acceptance criteria).

### 29.1 Purpose

Replace the V1.64 flat 10-item sidebar with a two-tab information architecture (Creator | Orchestrator) with nested nav, footer profile switcher, and simplified daemon status bar. The reshape addresses author-reported defects 3 (menu IA) and 4 (footer profiles) as one coherent IA pass.

### 29.2 Sidebar — two-tab structure

The sidebar renders at `lg`+ (≥961px) as a fixed left panel. Below `lg`, the two-tab structure collapses to a top dropdown or pill scroller.

**Two top-level tabs** (composited as horizontal tab bar at the top of the sidebar):

| Tab | Top-level items | Nested children |
|-----|----------------|-----------------|
| **Creator** | Works | (per-Work navigation nest: Chapters, Findings) |
| | Creator | Memory, SOUL |
| **Orchestrator** | Runtime | Sessions, Schedule, Capabilities |
| | Strategies | (single entry: `/strategies` list → `/strategies/:presetId` canvas detail) |

**Excluded from sidebar**:
- **Connect** — Settings → **Connection** (`/settings/connection`); legacy `/connect` permanently redirects (V1.103 C1). Not a sidebar item. **Implement authority:** [settings-connection-section.md](../iterations/v1.103/specs/settings-connection-section.md).
- **Daemon status** — leaves sidebar; lives in the status bar (running = restart-icon only) and the main-banner (degraded/error).

The old 10-item flat `NAV_ITEMS` array is retired. Tab switch swaps the visible nav items; the footer Profiles row is always visible regardless of active tab.

### 29.3 Nested nav behaviour

- Top-level items expand/collapse nested children.
- Per-Work nested nav (Chapters, Findings) is contextual to the currently active Work.
- Creator nested nav (Memory, SOUL) is static per the creator scope.
- Runtime nested nav (Sessions, Schedule, Capabilities) is static.
- Strategies is a single top-level entry under Orchestrator — no nesting; clicking opens `/strategies` list.

### 29.4 Strategies unification

The existing `/presets` (list) and `/strategy` (canvas) routes collapse to:

- **`/strategies`** — list view (replaces `presets-page.tsx` as entry).
- **`/strategies/:presetId`** — canvas detail view (preserves V1.70–V1.75 canvas surface verbatim).

**Preservation invariant**: The full V1.70–V1.75 canvas surface is preserved verbatim at `/strategies/:presetId` — React Flow behaviour, write-boundary (strategy patch routes, graphRevision), conflict modals (strategy-flavored copy), non-spatial alternate views, keyboard paths, and all canvas-write DESIGN.md tokens. This is an entry-point reshape only; no canvas rewrite.

**Redirect compatibility**: `/presets` → `/strategies` (301/302); `/strategy` → `/strategies/:presetId` (requires a stored active preset ID or redirects to list). Deep links from V1.70–V1.75 must resolve.

### 29.5 Footer profile switcher

Rendered at the bottom of the sidebar (always visible regardless of active tab). Slack/Chrome-style horizontal row of Creator avatar icons.

**Interaction contract**:
- **Click / Enter/Space**: switches `active_creator_id`; dependent queries refetch.
- **Keyboard**: arrow-left/right to navigate avatars; Home/End for first/last; Esc closes any transient UI (modal, dropdown).
- **"+" CTA**: opens a lightweight create-Creator modal consuming the existing `POST /v1/daemon/creators` endpoint.
- **Single-Creator case**: exactly one avatar + "+". Clicking the single avatar is a no-op (no error toast). The "+" is the only call-to-action.
- **Persistence**: `active_creator_id` stored in `localStorage` (key: `nexus:activeCreatorId`) for browser; Tauri store equivalent for desktop. Restored on reload.
- **Avatar fallback**: initials (first character of `display_name`) or generic icon when no image; must be accessible (not color-only).

### 29.6 Daemon status bar simplification

The V1.64 5-state pill (`starting`/`healthy`/`degraded`/`stopped`/`error`) + always-enabled Start button is retired. The simplified contract:

- **Running/healthy**: status bar shows **only** a restart-icon button (no pill, no state text, no Start button).
- **Degraded/error/stopped/crash**: a **top-of-main-content banner** (`main-banner.tsx`, new) surfaces the failure with error detail + Restart CTA. Not a sidebar item.
- **Never**: enabled-while-broken Start button; silent hang during daemon startup.

The daemon status bar subscribes to `onDaemonStatusChanged` (existing Tauri event / daemon SSE).

### 29.7 Button contrast invariant

Recorded in repo-root [`DESIGN.md`](../../DESIGN.md) §Component Primitives/Button and [`DESIGN.dark.md`](../../DESIGN.dark.md):

> **Every button (or button-like element) with a dark, primary, or saturated background MUST use light/white text in both light and dark themes.**

The dark primary token fix: `dark:bg-brand-cyan dark:text-brand-deep-blue` → `dark:bg-brand-cyan dark:text-white` (matching hover/active). The `DESIGN.dark.md` frontmatter `components.button.primary.textColor` changes from `"{colors.brand-deep-blue}"` to `"#ffffff"`.

P1 implements a sweep audit of all button call sites in `apps/web/src/**` for the dark-bg → light-text invariant. Vitest snapshot coverage (light + dark themes) gates regressions.

### 29.8 Browser-build contract

The wizard and per-launch daemon-ready gate are **desktop-first**:
- **Desktop (Tauri)**: `setup_completed` read from Tauri command `get_setup_completed()`; wizard renders when false **after** the V1.105 fullscreen Daemon gate reaches Ready (see §29.13).
- **Browser**: defaults `setup_completed = true` (i.e. no wizard). The daemon-ready gate is a no-op or instant pass. No Tauri command calls are assumed in the browser build. The browser SPA must not regress — existing Vite dev / static-serve flows continue unchanged.

### 29.9 Non-goals

- Full multi-creator CRUD / profile management (rename, delete, avatar upload) — footer switcher only selects existing or creates via existing endpoint.
  > **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-34 (multi-creator CRUD / multi-workspace UI / workspace switcher).
- Multi-workspace UI / workspace switcher.
  > **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-34 (multi-creator CRUD / multi-workspace UI / workspace switcher).
- Agent detection during non-first-launch.
- Mobile full rewrite (the `<lg` collapse preserves the two-tab structure as a dropdown/pill scroller but is not optimized for touch-first patterns).

### 29.10 V1.95 Amendments (historical — superseded by §29.13 for current wizard)

> **Current wizard IA and chrome:** §29.13 (Agent → Workspace → Done; portrait card; top horizontal Steps; app-level `DaemonLaunchGate`). This section records V1.95 shipped behavior for traceability only.

#### 29.10.1 Setup wizard layout redesign (V1.95 shipped behavior)

The setup wizard moves from a centered card with horizontal steps at the top to a left‑sidebar vertical step indicator with content on the right (V1.95 delivery):

- Steps: Welcome (workspace selection), Daemon (status/error/reset), Agent (detection/selection), Done.
- The wizard fills the entire window (no `min-h-screen items-center justify-center`).
- Step indicators are a vertical list in a fixed left panel (`w-52`), with the current step highlighted.
- Content area keeps the card chrome (border, shadow, background).

**Note**: V1.96 reworks the wizard to a centered, integrated single-card IA (see §29.11). The V1.95 description is retained for historical traceability only.

#### 29.10.2 Setup wizard workspace selection with native directory picker

Step 1 (Welcome) now includes a native directory picker (Tauri `@tauri-apps/plugin-dialog` `open({ directory: true })`) to let the user select a custom workspace path:

- Default workspace path: `~/Documents/nexus/default` (brand `nexus/`, not `nexus42/`; system home remains `~/.nexus42/`).
- Stale path overwrite: if the existing `workspace_path` matches `~/Documents/nexus42/default` or `~/Documents/nexus/local/default`, it is overwritten with the new default; custom user-set paths are preserved.
- Browser build hides the directory picker button (no native dialogs).

#### 29.10.3 FingerprintGate setup route bypass

The `FingerprintGate` adds `/setup` to its bypass routes (alongside `/connect`), so the wizard can render before any remote config exists without timing risks.

#### 29.10.4 ClientProvider immediate TauriClient for desktop

On desktop builds, `ClientProvider` returns `TauriClient` + `TauriDesktopCapabilities` immediately in the `!loaded` branch (no temporary `BrowserClient`), avoiding the "Request failed: The string did not match the expected pattern" error from same‑origin `/v1/daemon/runtime/health` calls in the Tauri webview.

#### 29.10.5 Daemon error surfacing + migration‑mismatch recovery

- Wizard step 2 (Daemon) surfaces the real error detail from `SidecarManager` (not a generic message).
- When the daemon fails to start (e.g., migration checksum mismatch), the wizard offers an **opt‑in "Reset local database" button** that clears the daemon state in `~/.nexus42/` (no user creative files touched) and retries daemon start.
- The button copy clearly states: "This will clear the daemon's local state database (config, registry cache). Your creative files in the workspace are not affected."

### 29.11 V1.96 Amendments — Setup Wizard Surface rework & daemon diagnostic chain (IA)

> **Layout supersession (V1.105):** §29.13.3 portrait shell + top Steps replace the V1.96 left-rail centered card. **Daemon supersession (V1.105):** diagnostic UX moves to `DaemonLaunchGate` / splash — not wizard step 2. Patterns below (toast errors, bottom CTA, Browse adjacency) remain applicable where noted in §29.13.

**Product behavior (author-visible).** These describe the IA and interaction patterns after V1.96. Token names, component implementation, and daemon-side capture details live in DESIGN.md and the implement plan, not here.

#### 29.11.1 Centered integrated card IA

- The wizard viewport is centered (horizontal + vertical) rather than left-aligned or edge-to-edge fill.
- Step indicator list and step content share a **single card chrome** container (one bordered, shadowed, background block). They are not separate sidebar + content panels.
- Within each step indicator row the marker (circle/number) and label text sit on the same horizontal baseline.

#### 29.11.2 Inline workspace location row (Step 1 — historical; V1.105 Workspace step 2)

- Workspace location is rendered as one inline affordance group:
  - Icon + label + resolved path + Browse button appear together on a single row (or tightly coupled rows inside the same visual block).
- The Browse control is visually adjacent to the path value (strong location-to-action association).

#### 29.11.3 Unified toast error surface + shared helper

- Tauri invoke errors (picker, workspace persist, daemon status, finish, etc.) **never** render as inline `<p role="alert">` text inside a step.
- All such errors surface via the page-level toast (error variant).
- A shared `errorMessage(err)` helper (consumed by every wizard step) turns Tauri `{ message }` objects, `Error` instances, and strings into readable text. The literal `[object Object]` string does not appear.

#### 29.11.4 Primary bottom CTA pattern

- Each step places its primary action ("Continue", "Finish", …) as a wide, prominent button at the bottom of the card content area.
- The secondary "Back" action is a smaller/tertiary button placed next to the primary.
- The pattern is consistent across all four steps.

#### 29.11.5 Daemon diagnostic UX (Step 2)

- The wizard does not remain stuck in "Starting daemon…" after subscription.
- The status callback explicitly handles the `'starting'` state (treated as in-progress).
- A hard timeout (≤30 s) fires a "Taking longer than expected" state exposing Retry / Reset actions if no terminal state arrives.
- On `error`, the `detail` shown contains the **real stderr** captured from the sidecar (clearly distinguishable from generic wrapper text).
- V1.95 fixes (immediate TauriClient, Reset local database, stale workspace handling) remain effective.

#### 29.11.6 Preservation

All V1.95 amendments (ClientProvider, migration reset, workspace default rules, FingerprintGate bypass) continue to apply. V1.96 adds the centered-integrated IA and the diagnostic surfacing improvements.

### 29.12 V1.97 Amendments — First-launch reliability hardening

> **Daemon step references below are historical.** V1.105 retires the Daemon wizard step; readiness is the app-level `DaemonLaunchGate` (§29.13.1). Workspace-location and Browse rules apply to the Workspace step (step 2) in the V1.105 IA.

**Product behavior (author-visible).** V1.97 keeps the V1.96 wizard IA but hardens the first-launch path so a new author either completes setup or reaches a bounded, actionable recovery state.

- Step 1 remains visually calm at desktop window sizes: the step list does not crowd content, the card does not overflow the viewport or right edge, and long workspace paths truncate inside the workspace location affordance.
- Browse remains a desktop-only native picker affordance and must pass the expected `defaultPath` argument to the Tauri command. If the picker cannot open, the user sees a readable toast/error, not a raw missing-key failure.
- The daemon step never treats indefinite progress as success. Clean-state launch must reach `running`, an actionable `error`, or a visible timeout/retry/reset state within the bounded V1.96 timeout behavior.
- Existing-install launch preserves prior setup guarantees: `setup_completed` skip behavior, workspace path preservation unless stale, reset-local-database recovery, and daemon stderr/diagnostic visibility.
- V1.97 does not add new onboarding steps, settings surfaces, daemon API fields, schema/contract changes, signing/update flows, or default-path consolidation work.

#### 29.12.1 Implementation invariants

- The workspace-location row must be flex-safe at desktop window sizes: content containers that hold the resolved path can shrink, and the path truncates inside the card instead of expanding the right edge.
- The native Browse path uses the existing desktop capability/IPC boundary. The frontend sends `defaultPath` to the Tauri command and does not introduce a second argument shape or compatibility shim.
- The daemon step observes existing desktop daemon-status state and `detail` only. It must not require new daemon API fields, generated schemas, or contract package changes.
- Clean-state smoke and existing-install smoke are hard product verification gates, not new UI features. Their evidence may be captured manually or with automation, but unit tests alone do not prove the author-visible first-launch path.

### 29.13 V1.105 Amendments — First-launch wizard reshape (Agent-first + app-level Daemon gate)

**Product behavior (author-visible).** V1.105 separates daemon readiness from wizard steps and reduces the wizard to three author-facing choices. **Iteration SSOT:** [`.mstar/iterations/v1.105/delivery-compass.md`](../iterations/v1.105/delivery-compass.md) + [`v1.105/specs/`](../iterations/v1.105/specs/).

#### 29.13.1 App-level fullscreen Daemon gate (not a wizard step)

- **Every desktop launch** — first-launch and return visits — shows a fullscreen Daemon wait until Ready before `/setup` or main UI.
- **Outer gate:** `DaemonLaunchGate` wraps `AppRoutes` in `apps/web/src/App.tsx`; renders `DaemonReadySplash` until Ready on desktop.
- **Inner gate:** `SetupGate` on main-shell routes only — after Ready, `setup_completed=false` → `/setup`; `true` → main UI. Splash logic **removed** from `SetupGate` post-P0.
- Desktop **always** auto-starts the bundled sidecar on app open (D2 — `apps/desktop/src-tauri/src/lib.rs` `.setup()` unconditional `SidecarManager::start`).
- The **Daemon wizard step is retired**. Diagnostic UX (timeout, retry, stderr detail, reset-local-database recovery) moves to the splash/gate surface — not a numbered setup step.
- Happy path does **not** use `startDaemon` IPC from the wizard; recovery only on splash error paths.

#### 29.13.2 Three-step wizard IA (Agent → Workspace → Done)

| Step | Step ID | Author-facing label | Module |
|------|---------|---------------------|--------|
| 1 | `agent` | Agent | `setup-step-agent.tsx` |
| 2 | `workspace` | Workspace | `setup-step-workspace.tsx` (new; extract from Welcome) |
| 3 | `done` | Done | `setup-step-done.tsx` |

Orchestrator: `setup-wizard-page.tsx` — `WizardStep = 'agent' | 'workspace' | 'done'`; initial step `agent`.

**Removed:** Welcome step (`setup-step-welcome.tsx`); Daemon step (`setup-step-daemon.tsx`).

Agent scan remains `POST /v1/daemon/agent-host/scan` via `useScanAgents` after gate Ready (grill-me **B**). Five scan-safety constraints per §desktop-shell 14.3. No Tauri-side PATH probe duplicate.

Bootstrap (`ensureSetupBootstrap`) on Workspace **Continue** only.

#### 29.13.3 Portrait wizard shell

- Fixed portrait card: **`480px`** max width (`--color-setup-wizard-wizard-max-width`), **`min(720px, 85vh)`** height (`wizard-max-height: 720px` + `max-h-[85vh]`); content scrolls inside the card (`flex-1 overflow-y-auto` on step body).
- Top horizontal `TopStepIndicator` (Agent / Workspace / Done); **no** left `w-setup-wizard-surface-step-panel-width` (208px) rail.
- Studio visual SSOT: `apps/design-studio/src/fixtures/setup-wizard-chrome-fixtures.tsx` before App wiring.
- V1.96 centered-card patterns (toast errors, bottom CTA, Browse adjacency) preserved where applicable — only layout chrome changes in P2.

#### 29.13.4 Settings Re-run Setup (V1.103 R1 compatibility)

- Settings → Setup → **Re-run Setup** semantics unchanged: confirm clears `setup_completed` marker only; workspace path and agent profile **not** deleted.
- After confirm, author passes the V1.105 fullscreen gate, then enters the **new** three-step wizard (Agent-first — not legacy Welcome-first).
- **Implement authority for re-run action:** [settings-setup-section.md](../iterations/v1.103/specs/settings-setup-section.md).

#### 29.13.5 Browser-build contract (unchanged)

§29.8 browser defaults remain: wizard and daemon gate are desktop-first; browser build skips wizard and gate.

#### 29.13.6 Non-goals

- Tauri PATH agent scan; multi-workspace switcher; BYOK; Settings shell IA redesign; wire/schema changes unless P0 proves unavoidable.
  > **Durable roadmap:** consolidated in the [deferred-features tracker §2.6](../knowledge/deferred-features-cross-version-tracker.md) — DR-35 (Tauri PATH agent scan), DR-34 (multi-workspace switcher + Settings shell IA redesign). BYOK: tracked as **DF-70** ([tracker §2.3](../knowledge/deferred-features-cross-version-tracker.md)).

### 30. V1.98 Amendments — Design Studio dev surface (not author-facing)

**Product classification.** `apps/design-studio` is a **contributor/dev auxiliary app** — a read-only gallery for the unified DESIGN SSOT, brand VI, and `apps/web` UI primitives. It is **not** part of the local Web UI product surface authors use. Authors do not receive a Design Studio nav item, route, or menu entry in Control Room, Setup, or desktop shell.

**Normative spec:** [`design-studio.md`](design-studio.md) · **IA:** [design-studio-information-architecture.md](../iterations/v1.98/guides/design-studio-information-architecture.md) · **Merge rules:** [design-unification.md](../iterations/v1.98/specs/design-unification.md) · **Compass:** [v1.98/delivery-compass.md](../iterations/v1.98/delivery-compass.md).

#### 30.1 DESIGN SSOT move (web consumer)

- After V1.98 merge, **repo-root** [`DESIGN.md`](../../DESIGN.md) + [`DESIGN.dark.md`](../../DESIGN.dark.md) are the sole normative DESIGN pair.
- Former `apps/web/DESIGN.md` and `apps/web/DESIGN.dark.md` are **deleted**; `src/index.css`, `tailwind.config.ts`, and AGENTS references consume the root SSOT via `@nexus/design-tokens`.
- Token **names** preserved verbatim where possible to minimize CSS churn; value changes from merge audit are documented in [`design-unification.md`](../iterations/v1.98/specs/design-unification.md) §9.
- `apps/web` behavior and author-visible UI remain the product contract in §§1–29; only the token **source path** changes.

#### 30.2 What design-studio is (and is not)

| Dimension | `apps/web` (this spec) | `apps/design-studio` |
| --- | --- | --- |
| Audience | Authors on local product | Contributors / frontend devs |
| Serving | Daemon rust-embed + Tauri bundle | Standalone `pnpm dev` only |
| Data | Daemon API / `NexusClient` | Static fixtures; no wire contracts |
| DESIGN role | Consumer | Read-only mirror + gallery |
| Shipped in `nexus42` | Yes | **No** |

#### 30.3 Author invariants (unchanged)

- No new screens, routes, or settings in the author Web UI for design-studio.
- Setup wizard, Control Room IA (§29), and daemon status behavior unchanged by studio work alone (product setup polish continues under active iteration contracts — see [V1.105 compass](../iterations/v1.105/delivery-compass.md)).
- Desktop first-launch / setup wizard reshape is owned by **V1.105** (P0 gate + P1 IA + P2 portrait shell); Design Studio remains a contributor gallery and does not deliver author onboarding.

#### 30.4 Contributor workflow (cross-reference)

Token tuning: edit repo-root [`DESIGN.md`](../../DESIGN.md) pair on disk → refresh design-studio → validate gallery → verify `apps/web` test/build. Full steps in [`design-studio.md`](design-studio.md) §4.2.

#### 30.5 Non-goals

- Bundling design-studio into daemon static assets or desktop installer
- Exposing studio URL from `nexus42 daemon ui` or author-facing docs as a product feature
- Live token editor or YAML write-back from studio UI
- Unbounded migration of `components/ui/*` into `@42ch/nexus-ui`; V1.99 handles only approved pure presentational primitives through the component-promotion boundary.
- `wire_contracts_changed: true` — V1.98 is frontend/docs only

### 29.14 V1.106 Amendments — Studio-first pipeline + first-launch polish

**Iteration SSOT:** [`.mstar/iterations/v1.106/delivery-compass.md`](../iterations/v1.106/delivery-compass.md) + [`v1.106/specs/`](../iterations/v1.106/specs/). **Implement authority** for Stretch Settings IA: [`ui-continuity.md`](../iterations/v1.106/specs/ui-continuity.md) (P2 — may defer).

#### 29.14.1 TopStepIndicator SSOT (P1 Must)

- **Single module:** `apps/web/src/components/setup/top-step-indicator.tsx` exports `WizardStep` and `TopStepIndicator`.
- **Studio:** imports `@web-setup/top-step-indicator` — no duplicate inline implementation in fixtures.
- Supersedes V1.105 note that Studio fixtures alone are visual SSOT for step indicator chrome.

#### 29.14.2 AgentPicker density (P1 Must)

- Shared `apps/web/src/components/setup/agent-picker.tsx` accepts optional `density?: 'default' | 'compact'` (default `'default'`).
- Settings omits prop; wizard may pass `compact` only. No second picker module.

#### 29.14.3 Settings Advanced IA (P2 Stretch)

When shipped: nav **Agent · Workspace · Advanced** only. Single `/settings/advanced` page with Connection + Setup sections (`id="connection"`, `id="setup"`). Legacy `/settings/connection`, `/settings/setup`, and `/connect` redirect to hash anchors on Advanced. **Normative detail:** [`ui-continuity.md`](../iterations/v1.106/specs/ui-continuity.md) §FB-V1106-005.

#### 29.14.4 Contract boundary

Prefer `wire_contracts_changed: false`. Studio-first invariant locked for all author-facing chrome (compass + [`studio-first-invariant.md`](../iterations/v1.107/guides/studio-first-invariant.md); V1.107 carry-forward).

#### 29.14.5 Voice & Content — first-launch + daemon chrome (P0/P1 Must)

Normative copy lives in repo-root [`DESIGN.md`](../../DESIGN.md) (`### Launch & daemon status`, `### Done step copy`, `### States`). Iteration examples:

| Surface | Example copy |
|---------|----------------|
| DaemonReadySplash (waiting) | **Starting daemon…** — *This takes a few seconds on first launch.* |
| DaemonReadySplash (error) | **Daemon not ready** — **Restart Nexus** |
| Done step | **You're ready 🎉** — *Open Nexus to start writing. You can change settings anytime.* — **Open Nexus** |
| AgentPicker loading | *Scanning for local ACP agents…* |
| AgentPicker empty | **No agents found on PATH** |
| Settings nav (P2 Stretch) | **Agent** · **Workspace** · **Advanced** |

### 29.15 V1.107 Amendments — Studio UI tune + coverage hygiene

**Iteration SSOT:** [`.mstar/iterations/v1.107/delivery-compass.md`](../iterations/v1.107/delivery-compass.md) + [`v1.107/specs/studio-ui-tune.md`](../iterations/v1.107/specs/studio-ui-tune.md).

#### 29.15.1 Studio Tailwind content (FB-000)

Design Studio `tailwind.config.ts` must include `apps/web/src/components/setup/**`, `layout/presentational/**`, and `packages/nexus-ui/src/**` in `content` so wizard chrome and component matrices paint.

#### 29.15.2 Workspace path field SSOT (FB-008)

- **Module:** `apps/web/src/components/setup/workspace-path-field.tsx`
- **Exports:** `WorkspacePathField`, `WORKSPACE_PATH_FIELD_LABEL` (`Workspace folder`), `WORKSPACE_PATH_CHANGE_ACTION` (`Change Folder…`)
- **Consumers:** Settings workspace section, wizard workspace step, Studio fixtures — shared label/CTA; wizard `layout="wizard-stack"`

#### 29.15.3 Presentational gallery aliases (FB-013..015)

| Alias | Resolves to | Use |
|-------|-------------|-----|
| `@web-layout/*` | `apps/web/src/components/layout/presentational/*` | Shell sidebar, footer profiles, header/health chrome |
| `@web-settings/*` | `apps/web/src/components/settings/presentational/*` | ConnectDaemon + Setup section chrome |

Studio **must not** import routing-heavy `sidebar.tsx` or IPC-backed `connect-daemon-form.tsx` directly.

#### 29.15.4 Toast App adoption (FB-012)

V1.106 promoted Toast to `@42ch/nexus-ui` for Studio fixtures. V1.107 requires App `apps/web/src/lib/use-toast.tsx` to become a **thin re-export** — closes `R-V1106P0-001`. Do not claim App/package Toast parity until FB-012 lands.

#### 29.15.5 Contract boundary

Prefer `wire_contracts_changed: false`. Studio-first invariant: [`studio-first-invariant.md`](../iterations/v1.107/guides/studio-first-invariant.md) (V1.107 carry-forward).

### 29.16 V1.112 Amendments — Frontend i18n (shipped)

**Iteration SSOT:** [`.mstar/iterations/v1.112/delivery-compass.md`](../iterations/v1.112/delivery-compass.md) + [`v1.112/specs/`](../iterations/v1.112/specs/). Normative product detail for this slice lives in the iteration workspace until P5 merge.

**Implement authority (iteration workspace):**

| Plan | Spec | Scope |
| --- | --- | --- |
| P0 — i18n foundation + Appearance | [`i18n-foundation.md`](../iterations/v1.112/specs/i18n-foundation.md) | `i18next` + `react-i18next`; Settings **Appearance** → Language (`system` \| `en` \| `zh-CN`); caller-owned copy; user-facing-only catalogs |
| P1 — full user-facing UI migration | [`i18n-ui-migration.md`](../iterations/v1.112/specs/i18n-ui-migration.md) | Remaining **user-facing** `apps/web` chrome + `format.ts` / `Intl` active-locale wiring |

When shipped: Settings nav becomes **Agent · Workspace · Appearance · Advanced**; theme toggle stays in the header (not moved into Appearance). Developer-auxiliary surfaces (including `apps/design-studio`) remain out of catalog scope. `wire_contracts_changed: false`.

### 29.17 V1.118 Amendments — Creation peer groups + Canvas-first work shell

**Status:** Draft (V1.118) — architect plan **done** (2026-07-15); implement authority in iteration workspace until P5 merge.

**Iteration SSOT:** [`.mstar/iterations/v1.118/delivery-compass.md`](../iterations/v1.118/delivery-compass.md) + [`.mstar/iterations/v1.118/README.md`](../iterations/v1.118/README.md).

**Implement authority (iteration workspace):**

| Plan | Spec | Scope |
| --- | --- | --- |
| P1 — Creation peer groups | [`creation-peer-groups.md`](../iterations/v1.118/specs/creation-peer-groups.md) | Creator tab list mode: Works / Worlds / Memories peer groups |
| P2 — Canvas-first work shell | [`canvas-work-shell.md`](../iterations/v1.118/specs/canvas-work-shell.md) | `/works/:workId/*` — `WorkShellLayout`, `WorkRail`, drill-in retirement |

#### 29.17.1 Creation tab IA (P1 — list mode)

Supersedes V1.117 sidebar Creator meta-group (Outline + World KB + Memory mix).

| Creator tab group | Contents | Route |
| --- | --- | --- |
| **Works** | All Works + flat work rows (≤12) | `/works`, `/works/:workId` (P2 default: `/works/:workId/outline`) |
| **Worlds** | Single link | `/worlds` |
| **Memories** | Single link | `/memory` |

Outline and World KB are **not** Creation peer groups. Strategy remains under Orchestrator (`/strategies`).

**Merge order:** P1 lands before P2 on `iteration/v1.118`.

#### 29.17.2 Canvas-first work shell (P2 — `/works/:workId/*`)

- **Layout:** `WorkShellLayout` nested in `RootLayout` `<main>` — center canvas outlet + right `WorkRail` (280px at `lg`+).
- **Default:** `/works/:workId` → `/works/:workId/outline`.
- **Rail MVP:** Works list + metadata preview (`WorkSummary`); no manuscript snippet.
- **Responsive:** `<lg` → end-sheet drawer for rail.
- **Retired:** V1.117 `isDrillIn` / `drillInItems` as primary enter-work UX; Creator \| Orchestrator tabs stay visible inside work.

#### 29.17.3 Contract boundary

`wire_contracts_changed: false`. Studio-first for new shell chrome per repo-root `DESIGN.md`.

#### 29.17.4 V1.125 amendment — Worlds-first Creator sidebar (Draft)

**Status:** Draft (V1.125) — supersedes §29.17.1 **Creator tab list-mode peer groups only** (not §29.18 Canvas World-entry Timeline default).

**Iteration SSOT:** [`.mstar/iterations/v1.125/delivery-compass.md`](../iterations/v1.125/delivery-compass.md) + [`creation-world-first-ia.md`](../iterations/v1.125/specs/creation-world-first-ia.md).

| Creator tab group | Contents | Route |
| --- | --- | --- |
| **Worlds** (primary) | Worlds list | `/worlds` |
| **Works** | Works list | `/works` |

**Removed from Creator sidebar (V1.125):** Timeline peer group, Work Timelines group, Memories (→ Orchestrator first group per P1). `/timeline` deep links and canvas Timeline surfaces remain per §29.18.

### 29.18 V1.122 Amendments — Three-pillar pivot + Timeline-first Canvas IA (Draft)

**Status:** Draft (V1.122) — P0 spec refactor in flight on `iteration/v1.122`; P1 implement authority in iteration workspace until P5 merge.

**Iteration SSOT:** [`.mstar/iterations/v1.122/delivery-compass.md`](../iterations/v1.122/delivery-compass.md) (Three-Pillar Pivot: Harness · Canvas · Computable).

**Implement authority (iteration workspace):**

| Plan | Spec | Scope |
| --- | --- | --- |
| P0 — Three-pillar spec refactor | [`pillar-framing.md`](../iterations/v1.122/specs/pillar-framing.md) + [`timeline-canvas-architecture.md`](../iterations/v1.122/specs/timeline-canvas-architecture.md) | Canonize Harness/Canvas/Computable in STRATEGY + CONCEPTS; `CanvasSurfaceKind = "timeline"` Draft overlay; corpus pillar cross-refs |
| P1 — Timeline-first Canvas | [`timeline-hero-product-spec.md`](../iterations/v1.122/specs/timeline-hero-product-spec.md) + [`timeline-canvas-architecture.md`](../iterations/v1.122/specs/timeline-canvas-architecture.md) | Elevate Timeline to peer surface + World-building projection + **World-entry default = Timeline** |

#### 29.18.1 Canvas IA — World entry default flips to Timeline (P1)

V1.122 inverts the **World-entry** default so an author meets a World's history first, not its entity graph. **Work entry is unchanged.**

| Entry context | Route | Default surface (V1.122) | Prior default |
| --- | --- | --- | --- |
| **World entry** | `/worlds/:worldId` → **Timeline** (index redirect / `/timeline`); Worlds list pick-target updates | **Timeline (World-building hero)** | `/worlds/:worldId/kb` (World KB) |
| **Work entry** | `/works/:workId` → Outline | **Outline (Timeline-companion)** — **unchanged** (V1.118 §29.17.2) | Outline (V1.118) |

The Canvas shell now hosts **four** peer surfaces: Strategy / Outline (Timeline-companion) / **Timeline** / World KB. The `CanvasSurfaceKind` enum gains `"timeline"` as an additive peer value. The Outline (Timeline-companion) surface keeps its chapter-relative timeline lane (Work projection); Timeline (World-building hero) projects the World KB graph's `block_type=event` entities onto a when-axis. Surface contract: [`canvas-strategy-surface.md`](canvas-strategy-surface.md) §3.3.2 + §4.5.

#### 29.18.2 Pillar framing (P0)

The Web UI is the primary home of the **Canvas** pillar (spatial steering surface, with Timeline-centric World building as the hero). The **Harness** pillar (orchestration/agent host/capability registry) is surfaced today as "Strategy/Preset" — the product rename to "Harness" is deferred (`DF-V1122-HARNESS-RENAME`), so V1.122 UI strings keep "Strategy". The **Computable** pillar (WASM reactivity) is backend-only in V1.122; compute-registry/canvas surfacing is deferred (`DF-V1122-COMPUTABLE-UI`, `DF-V1122-COMPUTE-ON-TIMELINE`). Pillar definitions: [`STRATEGY.md`](../../STRATEGY.md) + [`CONCEPTS.md`](../../CONCEPTS.md).

> **V1.147 forward-pointer:** the Computable pillar is no longer backend-only —
> V1.147 shipped **Run Studio** on the Modules surface (`DF-V1122-COMPUTABLE-UI`
> closed) and **compute-on-Timeline** entry with Accept-landed **Compute result**
> nodes (`DF-V1122-COMPUTE-ON-TIMELINE` closed; both tracker rows archived).
> The direct lane routes: [`daemon-api-surface-conventions.md`](daemon-api-surface-conventions.md) §12.3.
> Product lock: [`computable-author-behavior.md`](../iterations/v1.147/specs/computable-author-behavior.md).

#### 29.18.3 Contract boundary

`wire_contracts_changed: false`. P1 is additive frontend only (new `CanvasSurfaceKind = "timeline"` enum value + new Timeline adapter module under `apps/web/src/components/canvas/timeline-canvas/`); no `schemas/`, no codegen, no daemon Rust change, no `@42ch/nexus-contracts` version bump. Timeline reads `WorldKbGraphResponse` (V1.73) and writes through `kb.patch_entity` (V1.73) only — full architect-locked contract in [`timeline-canvas-architecture.md`](../iterations/v1.122/specs/timeline-canvas-architecture.md).
