# Local Web UI (Control Room + Setup → Content-Authoring) — Specification v1

**Status**: Shipped (V1.65) — Control Room + Setup MVP (V1.64) **+ Content-Authoring UI stage (V1.65, §13)**: outline rich-text editor + chapter structure table + structure CRUD (slug/wc/volume/status; title display-only) + body read-only render + browser "Copy path" context menu. Tauri desktop shell + body full-text editor + "open-with" → **V1.66** (compass §0 Q5). QC tri-review Approve (fix-wave-1) + QA Pass. **+ V1.67 Surface Convergence & De-risk (§15)** + **V1.69 Design System Maturation & Canvas Draft** (`apps/web/DESIGN.md` Production + Canvas Draft) + **V1.70 Canvas Strategy Implement α (§16)** + **CI/desktop-build optimization** (parallel ops track; PR path filter narrowed + release-gated full build) + **V1.71 Canvas Strategy Write-Boundary (§17)** (Strategy patch routes, graphRevision conflict policy, conflict modal UX, canvas-write tokens) + **V1.72 Canvas Outline+Timeline β (§18)** (3 outline/timeline patch routes `outline.patch_structure` / `outline.patch_chapter` / `timeline.patch_event` + outlineRevision conflict policy + outline-flavored conflict modal UX + non-spatial alternate views + 8 outline/timeline canvas-write DESIGN.md tokens). V1.71 `wire_contracts_changed: TRUE` for Strategy; V1.72 `wire_contracts_changed: TRUE` for additive Outline+Timeline (`@42ch/nexus-contracts` 0.7.0 → 0.8.0); V1.73 `wire_contracts_changed: TRUE` for additive World KB (`@42ch/nexus-contracts` 0.8.0 → 0.9.0). **V1.74 Shipped** — Canvas World KB Relationships β (§20) with typed relationship edges, `world_kb.patch_relationship`, relationship inspector, non-spatial relationship table, and KB-flavored conflict modal reuse.
**Document class**: Feature line  
**Created**: 2026-06-24  
**Scope**: Nexus local Web UI product contract — placement (`apps/web`), stack, daemon-served model, `tauri-api` adapter boundary, MVP surface (Control Room + Setup), Content-Authoring stage (V1.65), Tauri / body-editor roadmap (V1.66), and strict separation from the private cloud SaaS  
**Iteration compass**: [v1.64-local-web-ui-kickoff-delivery-compass-v1.md](../../iterations/v1.64-local-web-ui-kickoff-delivery-compass-v1.md) (V1.64 ship) · [v1.65-outline-and-structure-authoring-delivery-compass-v1.md](../../iterations/v1.65-outline-and-structure-authoring-delivery-compass-v1.md) (V1.65 Content-Authoring stage) · [v1.69-design-system-maturation-and-canvas-draft-compass-v1.md](../../iterations/v1.69-design-system-maturation-and-canvas-draft-compass-v1.md) (V1.69 Design System Maturation & Canvas Draft — DESIGN.md Production migration + Canvas Exploration → Draft) · [v1.70-canvas-strategy-implement-and-ci-optimization-compass-v1.md](../../iterations/v1.70-canvas-strategy-implement-and-ci-optimization-compass-v1.md) (V1.70 Canvas Strategy Implement α + CI/desktop-build optimization — the first canvas surface ships) · [v1.71-canvas-strategy-write-boundary-and-hygiene-compass-v1.md](../../iterations/v1.71-canvas-strategy-write-boundary-and-hygiene-compass-v1.md) (V1.71 Canvas Strategy Write-Boundary β + hygiene companion)

**Coordinates with**:

- [cli-spec.md](cli-spec.md) §6.3 (daemon command group — Web UI access) + §7.1 (first-run path)
- [daemon-runtime.md](daemon-runtime.md) §2 (normative layering) — static-asset serving on the axum router
- [../schemas-external-consumer-boundary.md](../schemas-external-consumer-boundary.md) — the bundled UI is a first-class external consumer of `@42ch/nexus-contracts`
- [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) §1 — strict local-product vs cloud-product separation
- `apps/web/DESIGN.md` (NEW, project-level, `@architect`-authored) — design tokens this UI consumes
- [local-api-surface-conventions.md](local-api-surface-conventions.md) (NEW, `@architect`-authored Master) — cursor pagination / `ErrorResponse` / naming conventions the UI data layer relies on

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
| Data source | local `state.db` + reference store via loopback Local API | platform HTTP / cloud DB |
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
- **Dev** — `apps/web` runs the **Vite dev server**, which proxies `/v1/local/*` to the running daemon (`nexus42 daemon start`). No embedding in dev; hot reload against the live Local API.

The static shell (HTML/JS/CSS assets) is **unauthenticated** by design: it carries no data. All data flows through the Local API.

### 4.2 Auth model (unchanged from the daemon)

The Web UI introduces **no new auth surface**. It inherits the daemon's existing loopback model (V1.20 compass): Local API data endpoints are reachable on `localhost` and are **keyless on loopback**; the static shell needs no credential because it holds no data. The UI does not add login, sessions, or tokens. Any future remote (non-loopback) access is explicitly out of scope (§8) and would require its own auth spec.

> Implementation note for `local-api-surface-conventions.md`: the shared `ErrorResponse` (F-E1) is what the UI's toast/notification layer parses; the UI must never have to special-case per-handler error shapes.

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
                  /v1/local/*  (axum, hardened by Track B)
```

- **`BrowserClient`** (V1.64) — `fetch` against `http://localhost:<port>/v1/local/*`. This is the only shipped impl this iteration.
- **`TauriClient`** (V1.65) — implements the same interface via Tauri `invoke`; ships as a stub/interface-reference now so the boundary is frozen and P2 screens are transport-agnostic. Not implemented in V1.64.

The interface exposes the Local API resources the MVP consumes (conceptual — exact signatures are owned by plan P1, compass §5 item #7):

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
- **Findings-remediation UI** — V1.65+. Findings are *visible* in V1.64; acting on them is deferred.
- **Schedule / cron editor** — V1.65+. Hand-editing cron is a footgun for non-technical authors; schedules are driven by presets/CLI. V1.64 only displays them.
- **Mobile (Tauri v2 mobile targets)** — V1.66+.
- **OpenAPI spec generation + generated TS client SDK (C2)** — deferred; TanStack Query + codegen TS types suffice for the SPA.
- **Remote (non-loopback) access / any new auth** — out of scope; would require its own auth spec.
- **agent-host sessions/operations/events(SSE) DTO promotion** — long-lived stateful connections; remains deferred from V1.63.
- **F-P3 (rename list arrays to `items`)** + **F-F1 (`sort_by`/`sort_order`)** — adapter-covered in V1.64; structural closure V1.66+.

---

## 9. Roadmap

| Version | Scope |
| --- | --- |
| **V1.64** | Control Room + Setup MVP (browser SPA), daemon-served via `rust-embed`, `tauri-api` adapter boundary frozen. |
| **V1.65 (§13 stage)** | **Content-Authoring UI** (lead slice): outline rich-text editor + chapter structure table + structure CRUD + body read-only render + browser "Copy path"; **Track B** API hardening (chapter-content surface, `work_profile`, preset full CRUD, `items`+cursor). Tauri shell deliberately deferred to V1.66 (compass §0 Q1/Q5). |
| **V1.66** | **Tauri desktop shell** (`apps/desktop`) — loads `apps/web/dist`, system webview, `TauriClient` impl, daemon hosting (sidecar `nexus42 daemon start`); per-OS webview deps / signing / CI matrix. **"Open with" / "Reveal in file manager"** desktop integration (Tauri `shell.open`/`openWith`). Body full-text editor direction **rejected** (2026-06-26 — see §15.3); UI productivity deferred to V1.68. |
| **V1.67 (§15 stage)** | **Surface Convergence & De-risk**: Local API `items` array-key convergence + error-envelope consolidation (FE1-ORCH) + error-code casing + sort params (all transparent to the author); work_profile selector in Create-Work dialog; preset **TS-client promotion** (preset **management UI deferred to V1.68 canvas**). **Canvas Strategy Surface Exploration** authored (de-risk V1.68). Body editor direction **rejected** (§15.3). |
| **V1.69** | **Design System Maturation & Canvas Draft** (calm hygiene + V1.70 de-risk; no new UI screens, no wire change): `apps/web/DESIGN.md` migrated to **Production** completeness (YAML frontmatter SSOT + new `apps/web/DESIGN.dark.md` + body reshaped to rule-type docs + Level 3); **Canvas Exploration → Draft** (interface contracts + structured write-boundary + canvas→DESIGN.md token contract); 4 V1.67 frontend refactor residuals closed (work_profile literal union, `WORK_PROFILES` SSOT module, adapter-contract parity, preset query keys). Token names preserved verbatim → zero `index.css`/`tailwind.config.ts` consumer changes. |
| **V1.70 (§16 stage)** | **Canvas Strategy Implement (α)** — the first canvas surface ships: shared Canvas Shell (`@xyflow/react`, route-split behind `/strategy`) + Strategy (Preset) graph read projection (preset YAML → outer state-machine nodes + inner-DAG sub-flows + Converge join nodes per Draft §3.2) + live execution overlay (session-level status, 5 s poll) + Idea-input affordance + Run/Resume/Steer verbs (reuse existing schedule/orchestration endpoints; `wire_contracts_changed: FALSE` — A5 verdict: option a, client-side YAML parse from existing `getPreset(id).yaml`; no new backend route). DESIGN.md canvas tokens filled with concrete light + dark values. Accessibility baseline (keyboard-first, non-spatial alt view, ARIA live-region summary, focus management). UI copy adopts **Strategy** terminology; persisted identifiers remain `preset`. **`R-V167PSEC-QC1-S-UNMOUNT`** closed (daemon-status-bar subscription-cleanup race fix alongside overlay work). **Parallel ops track**: `desktop-build.yml` PR path filter narrowed (Rust-only PRs skip the 75 min macOS packaging build; main + tag/release triggers retain full coverage); new `desktop-release.yml` for distributable artifacts; **`R-V167PSEC-QC1-S-CI-SETUO`** closed (`set -euo pipefail`); `ci.yml` untouched. |
| **V1.71 (§17 stage)** | **Canvas Strategy write-boundary β** — Strategy patch routes + graphRevision tracking + conflict modal UX + canvas-write DESIGN.md tokens. Desktop signing groundwork is companion ops scope; outline+timeline and World KB remain future surfaces. |
| **V1.72 (§18 stage)** | **Canvas Outline+Timeline β** — 3 patch routes (`outline.patch_structure` / `outline.patch_chapter` / `timeline.patch_event`) + outlineRevision frontmatter tracking + outline-flavored conflict modal UX (use `{node_label}` placeholder adapting to chapter/event/volume domain entity) + non-spatial alternate views (sortable chapter list + sortable timeline event list) + 8 outline/timeline canvas-write DESIGN.md tokens (`canvas-outline-volume-fill` + 4 chapter-card statuses + `canvas-outline-timeline-event-pin` + `canvas-outline-foreshadow-edge` + `canvas-outline-timeline-marker` + `canvas-outline-conflict-marker`) + atomic outline markdown persistence (body preserved under `RuntimeLockGuard`). **Body ownership invariant**: outline markdown body remains V1.65 editor-owned and is never overwritten by canvas writes. **Companion hygiene + release hardening**: per-inspector save split (R-V171P0-QC1-004 HIGH) + strategy-canvas.tsx 7-module split ≤200 lines (R-V171P0-QC1-006 MEDIUM) + desktop-release.yml signing workflow completion (keychain + notarize + staple + unsigned fallback on signing failure; R-V171-CI-RELEASE-WORKFLOW-INCOMPLETE MEDIUM) + CI setup composite action (R-V171-CI-WORKFLOW-SETUP-DEDUPE LOW). Cmd/Ctrl+S save-trigger replay fixed via `lastHandledTriggerRef` edge-trigger. |
| **V1.73 (§19 stage)** | **Canvas World KB β** — third canvas surface (after Strategy α/β + Outline+Timeline β): 2 structured patch routes (`world_kb.patch_entity` for entity title/body/aliases/block_type edits + `world_kb.promote_candidate` for adopt/reject/merge promotion state machine) + per-row OCC conflict detection (reuses `kb_key_blocks.revision` + `kb_extract_jobs.version`; 409 `WorldKbConflictError` + 422 `WorldKbValidationError`) + Canvas UI: World KB graph projection (entity nodes + promotion-state badges + source-anchor edges + computable badges) + entity inspector + promotion inspector + conflict modal (KB-flavored copy) + non-spatial alternate view (sortable entity list with virtualization) + backend prerequisite: promoted World KB read+write from CLI-direct-DB to 4 first-class Local API routes + `@42ch/nexus-contracts` 0.8.0 → 0.9.0 (additive World KB DTOs) + 17 `canvas-worldkb-*` DESIGN.md tokens. Track B companion: 4 outline β hardening (MEDIUM validation gaps) + outline-canvas.tsx split + 2 release-hardening items. |
| **V1.74 (§20 stage)** | **Canvas World KB Relationships β** — fourth Canvas World KB capability: typed relationship edges, `world_kb.patch_relationship`, relationship inspector, non-spatial relationship table, conflict modal reuse, and relationship DESIGN.md tokens. Hygiene slate cleared in the same iteration. |
| **V1.75 (§21 stage)** | **Canvas-Pivot** — the V1.65 §13 whole-document TipTap outline editor (`chapter-page.tsx` Outline tab + `usePutChapterOutline` PUT save path) is **retired**. The V1.72 node-granular canvas is now the **sole outline authoring surface**. The canvas chapter inspector gains outline-prose TipTap editing via a new `content` field on `outline.patch_chapter` (parity-close — the inspector replicates the V1.65 editor's rich-text capability: headings, bold, italic, lists, markdown round-trip via `tiptap-markdown`). The retired `chapter-page.tsx` morphs to a read-only body view + "Edit outline → Canvas" redirect CTA — the reading/preview value is preserved (body prose render, frontmatter metadata strip, Copy Path), and outline authoring is relocated (not lost) to the canvas. This is a pre-1.0 hard cutover: no dual-editor deprecation period; the pivot is a clean retire+replace. `wire_contracts_changed: TRUE` (`content` field + V1.65 PUT write route/DTO removal → `@42ch/nexus-contracts` 0.10.0 → 0.11.0). |
| **V1.76 (§22 stage)** | **World KB Relationship γ — auto-extraction + confidence** — completes the World KB relationship surface. **Extraction proposes relationships**: `nexus.llm.extract` emits relationship candidates (entity pairs + relation_type + confidence + source anchors) from chapter text, persisted behind a `needs_review=1` gate (`source='extraction'`); the canvas shows them in a **Suggested pane** (sortable by confidence, default high→low, per-row Promote/Delete + bulk Promote all). **Confidence-weighting UX**: graph edges render with stepped confidence bands (low <0.4: 1px/30%, mid 0.4–<0.7: 2px/60%, high ≥0.7: 3px/100%) consuming the shipped DESIGN.md `canvas-worldkb-relationship-confidence-*` tokens; confidence-band colored badges (red/amber/green, uniform 8px) appear on edge labels. **`needs_review` gate semantics**: extraction defaults to `needs_review=1`; GET graph defaults to excluding suggested rows (`?include_suggested=true` surfaces them); suggested edges render dashed (distinct from confirmed solid). **Curation**: promotion clears `needs_review` via the existing `world_kb.patch_relationship` route (`needs_review: false` on update); `source` stays read-only provenance. `wire_contracts_changed: TRUE` (additive `needs_review` + `source` + extraction DTO + `include_suggested` → `@42ch/nexus-contracts` 0.11.0 → 0.12.0). No new DESIGN.md colors — the stepped bands reuse shipped confidence tokens. Desktop distribution v2 actual signing rollout remains blocked on Apple Developer ID cert + notarization credentials; Mobile (Tauri v2 mobile targets) remains future scope. |

The Tauri-ready boundary (§5) is what keeps the V1.66 shell a thin wrap rather than a rewrite. The V1.68 canvas adds new screens (graph surfaces) on the unchanged transport boundary — not a re-architecture.

---

## 10. Wire contracts note

This Feature line adds **no new wire schemas** of its own. It consumes the V1.63-promoted local-api schemas and the Track-B hardening of V1.64:

- Depends on **F-E1** (shared `ErrorResponse`) for unified UI error handling.
- Depends on **F-P1** (Works cursor pagination) for the dashboard list.
- Depends on **F-P2** (findings list endpoint) for the findings view.
- Adapts around **F-P3** (list-array naming) and **F-F1** (sort) client-side until V1.66+.

Versioning, npm/Rust bumps, and the single breaking shape change (Works list) are owned by compass §1.3 and `local-api-surface-conventions.md`.

---

## 11. CLI entry (summary; detail in cli-spec.md §6.3 amendment)

- `nexus42 daemon start` serves the UI at `http://localhost:<port>/` and **logs that URL** on startup.
- An optional `nexus42 ui` (alias `nexus42 web`) convenience command starts the daemon if not running and opens the OS browser. Whether it ships in V1.64 (P3) or is deferred is a PM decision grounded in cost; the spec records the outcome at P-last.

---

## 12. Acceptance (spec-level)

1. The UI is served from the `nexus42` binary (release) with no Node runtime requirement, and from the Vite dev server (dev) proxying `/v1/local/*`.
2. All seven MVP screen groups render and operate against the hardened Local API; no screen calls a transport directly (all via `NexusClient`).
3. The `tauri-api` adapter boundary is frozen: `BrowserClient` ships, `TauriClient` exists as a documented stub/interface reference.
4. Errors surface as one parsed `ErrorResponse` shape across all screens.
5. No cloud-product / platform-gated feature appears in the UI while `platform_integration = paused`.
6. The UI consumes `@42ch/nexus-contracts` via `workspace:*` with zero handwritten duplicate wire types.
7. V1.65 Tauri shell is achievable by implementing `TauriClient` and wrapping `apps/web/dist` — no screen rewrite.

---

## 13. Next stage — Content-Authoring UI (V1.65 lead slice)

> **V1.75 Canvas-Pivot note:** the V1.65 whole-document TipTap outline editor described in this section (the `chapter-page.tsx` Outline tab + `usePutChapterOutline` PUT save path) is **retired** as of V1.75 (§21). Outline authoring now happens exclusively on the V1.72 node-granular canvas; `chapter-page.tsx` is now a read-only body view + "Edit outline → Canvas" CTA. The chapter structure table + body read-only render behaviors below remain accurate; only the outline *editor surface* moved to the canvas. See §21 for the pivot stage.

V1.64 made the runtime **legible and configurable** (Control Room + Setup). V1.65 takes the next step: the UI becomes an **authoring entry surface** — authors can plan, review, and restructure chapter **outlines and structure** directly in the browser, with the chapter **body rendered read-only**. This is the single highest-leverage product-completeness move after V1.64: the runtime is feature-complete for writing and now UI-reachable, but the UI cannot yet *shape* the writing — only observe and configure it.

> **Scope and roadmap SSOT**: [v1.65-outline-and-structure-authoring-delivery-compass-v1.md](../../iterations/v1.65-outline-and-structure-authoring-delivery-compass-v1.md) §0 (grill decisions) + §1.1 (Track A) + §1.2 (V1.66 roadmap) + §5 (open design items). This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 13.1 What ships in V1.65 (Track A lead slice)

The browser SPA gains an authoring surface layered on the V1.64 Control Room + Setup screens. All new screens route through the same `NexusClient` interface (§5) and consume the new V1.65 chapter-content Local API (Track B / P0 backend; conventions in [local-api-surface-conventions.md](local-api-surface-conventions.md)).

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

The authoring surface consumes new chapter-content schemas (additive, owned by Track B / P0; conventions in [local-api-surface-conventions.md](local-api-surface-conventions.md)): chapter list (cursor + `items`) / detail / outline GET+PUT (atomic write) / structure PATCH (status progression) / body GET (read-only), plus `work_profile` on Work requests and full preset CRUD routes. Versioning, npm/Rust bumps, and per-DTO `schema_version` increments are owned by compass §1.3.

---

*Local-first Web UI product contract. V1.64 Shipped (Control Room + Setup); V1.65 §13 Content-Authoring stage amendment promotes at V1.65 P-last. Design tokens: `apps/web/DESIGN.md` (V1.64 Standard + V1.65 Standard+ editor/table/context-menu increment); design intent input: [web-ui-design-requirements.md](web-ui-design-requirements.md).*

---

## 14. Next stage — Desktop Shell (V1.66 lead slice)

V1.65 made the UI an **authoring entry surface** in the browser. V1.66 takes Nexus from **"open a browser tab to `localhost:8420`"** to a **double-clickable macOS desktop application**. The browser SPA transport stays **unchanged** (screen data access remains transport-agnostic); a new `apps/desktop` Tauri v2 wrapper loads the `apps/web` dist, the `TauriClient` impl of `NexusClient` swaps in, and the bundled `nexus42` daemon comes up transparently on launch. This is the gating prerequisite for everything desktop-native in the roadmap (signing, multi-OS, auto-update, mobile).

> **Scope and roadmap SSOT**: [v1.66-tauri-desktop-shell-delivery-compass-v1.md](../../iterations/v1.66-tauri-desktop-shell-delivery-compass-v1.md) §0 (grill decisions Q1/Q2) + §1.1 (Track A) + §1.2 (V1.67+ roadmap) + §5 (locked design items). Contract detail: [desktop-shell.md](desktop-shell.md). This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

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

V1.66 shipped the Tauri desktop shell. V1.67 is a **hygiene-lead consolidation & de-risk** iteration: it converges the Local API surface to one error envelope + one array-key convention + casing discipline, closes ~26 residuals, polishes the just-shipped desktop shell, closes the work-profile selector gap, and authors the **Canvas Strategy Surface Exploration** that de-risks the V1.68 lead. **No new author-facing features ship** — the only user-visible change is a work-profile selector in the Create-Work dialog. The canvas *implement* is V1.68; V1.67 authors its *design* only (see §15.3). *(Revised 2026-06-26: the prior body-editor lead was rejected — Nexus is an AI-autonomous executor; the AI owns prose, the human steers via Canvas.)*

> **Scope and roadmap SSOT**: [v1.67-local-api-surface-convergence-and-derisk-delivery-compass-v1.md](../../iterations/v1.67-local-api-surface-convergence-and-derisk-delivery-compass-v1.md) §0 (grill decisions + 2026-06-26 re-discussion Q4–Q6) + §1.1 (Tracks A–F) + §1.2 (V1.68 roadmap) + §5 (locked design items). This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 15.1 What ships in V1.67 (author-visible surface)

One small but unlocking UI change, a transport-only client promotion, plus a transparent API convergence the author never sees.

- **Work-profile selector in the Create-Work dialog** (G1): when an author creates a new Work, the dialog now includes a profile selector offering `novel`, `essay`, `game-bible`, and `script`. The wire contract already carried `work_profile` since V1.65 (additive optional field); V1.67 surfaces it in the UI. An author who skips the selector gets the default (`novel`) — no behavior change from V1.66. This is a prerequisite for the V1.68 canvas, which will tailor the steering surface per profile.
- **Preset CRUD TS-client promotion** (G2, transport half only): the daemon already ships `GET/PATCH/DELETE /v1/local/presets/{id}` + contracts; V1.67 promotes the 3 methods on the TS `NexusClient` interface (21 → 24) so the V1.68 canvas Strategy surface can consume them. **No form-based preset-management UI is built in V1.67** — the canvas Strategy surface supersedes a form UI (§0 Q6); building one now would be throwaway.
- **`items` array-key convergence** (transparent to authors): all schema-backed list responses now use `items` (previously `works`/`schedules`/`sessions`/`capabilities`). Pre-1.0 breaking wire change (see §15.5), but the author experiences nothing — the UI data layer adapts internally.

### 15.2 The de-risk loop this enables

V1.67 does not add an authoring loop; it *unblocks* the V1.68 canvas loop and *cleans* the foundation it builds on:

1. **Profile up-front** — an author starting a Work picks its profile at creation time, so the V1.68 canvas can tailor the steering surface per profile without a retrofit.
2. **Preset transport ready** — the TS client can already get/update/delete presets, so the V1.68 canvas Strategy editor wires directly to the daemon surface (no transport gap to close mid-canvas-build).
3. **Consistent API surface** — the V1.68 canvas (a heavy Local API consumer — graph nodes bind to lists/details) builds on a Local API with one error envelope, one array-key convention, and casing discipline — not the ad-hoc shapes V1.64 left behind.

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

> **Scope and roadmap SSOT**: [v1.70-canvas-strategy-implement-and-ci-optimization-compass-v1.md](../../iterations/v1.70-canvas-strategy-implement-and-ci-optimization-compass-v1.md) §0 (grill decisions Q1–Q3) + §1.1 (Tracks A+B) + §1.2 (V1.71+ roadmap) + §5 (locked design items). This section records the product contract; the compass is authoritative for scope, batching, and residual tracking. Architectural detail: [canvas-strategy-surface.md](canvas-strategy-surface.md) (V1.70 Shipped α — write-boundary + outline/timeline + World KB remain Draft V1.71+).

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
- **CLI / schema rename of `preset` → `strategy`** — breaking; deferred. V1.70 adopts **Strategy** terminology in UI copy only (Draft §4.2).
- **Desktop distribution v2** (signing / tri-OS / auto-update) — depends on external signing cert; remains V1.71+ backlog. V1.70 CI work is trigger/path optimization only, not signing.
- **Rich live overlay** (completed-path history, child-session hierarchy) — **V1.71** with the write-boundary contract.
- **Tauri WKWebView runtime smoke** — implementer documented they could not run Tauri locally; QA ran `cargo check` only. Full gesture/keyboard/pan-zoom validation inside actual WKWebView is a V1.71 follow-up if deeper runtime validation is needed.

### 16.6 User stories (V1.70 α slice)

- **Read the Strategy as a graph** — *As an author*, I see my Strategy (preset) rendered as a state-machine graph with visible join/wait nodes, so I understand how Nexus will execute my Work before it runs — and I can rewire a branch or adjust a gate on the canvas *(rewire/edit deferred to V1.71; V1.70 ships the read + overlay)*.
- **Steer by Idea** — *As an author*, I express an Idea (Work-level or on a specific node) and choose **Steer / Run / Resume / Ask Nexus to revise**, then Nexus executes — drafting prose, advancing the chapter, updating the KB — so I direct the work without typing the body myself.

(Outline chapters, World KB entities, and non-Strategy graph editing live in V1.72+ stories.)

---

## 17. Next stage — Canvas Strategy Write-Boundary (V1.71)

V1.70 made the Strategy canvas legible and steerable. V1.71 makes the **Strategy surface editable at node granularity** while preserving the core boundary: the browser/Tauri webview never writes raw files. All Strategy edits flow through schema-backed Local API patch routes, daemon validation, atomic persistence, and graphRevision conflict handling.

> **Scope and roadmap SSOT**: [v1.71-canvas-strategy-write-boundary-and-hygiene-compass-v1.md](../../iterations/v1.71-canvas-strategy-write-boundary-and-hygiene-compass-v1.md) §1.1 Track A (A1–A9), §1.3 wire contracts, §2 normative specs, and §6 risk notes. Architectural detail: [canvas-strategy-surface.md](canvas-strategy-surface.md) (V1.71 Shipped β) and [local-api-surface-conventions.md](local-api-surface-conventions.md) §7 patch-route pattern.

### 17.1 What ships in V1.71 (Track A — Strategy β writes)

- **Strategy patch routes**: the UI calls three new Local API routes through `NexusClient`, not `fetch`/Tauri filesystem access:
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

> **Scope and roadmap SSOT**: [v1.72-canvas-outline-timeline-beta-and-hygiene-compass-v1.md](../../iterations/v1.72-canvas-outline-timeline-beta-and-hygiene-compass-v1.md) §1.1 Tracks A+B, §1.3 wire contracts, §2 normative specs. This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 18.1 What ships in V1.72 (Track A — Outline+Timeline β)

- **3 outline/timeline patch routes**: the UI calls three new Local API routes through `NexusClient`, following the V1.71 Strategy patch-route convention:
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

V1.72 shipped the Outline+Timeline canvas. V1.73 completes the Canvas program's third surface — **World KB β** — the final of the three Draft canvas surfaces. The World KB canvas surfaces the author's accumulated world knowledge (characters, locations, items, events, organizations, conflicts, and computable derived entities) as a graph with entity nodes, promotion-state lifecycle badges, source-anchor provenance edges, and structured patch operations. This is the first canvas surface to require a **backend prerequisite**: promoting World KB read+write operations from CLI-direct-DB to first-class Local API routes with per-row OCC revision tracking.

> **Scope and roadmap SSOT**: [v1.73-canvas-world-kb-beta-and-outline-hardening-compass-v1.md](../../iterations/v1.73-canvas-world-kb-beta-and-outline-hardening-compass-v1.md) §1.1 Tracks A+B, §1.3 wire contracts, §2 normative specs. This section records the product contract; the compass is authoritative for scope, batching, and residual tracking.

### 19.1 What ships in V1.73 (Track A — World KB β)

- **2 structured patch routes**: the UI calls two new Local API routes through `NexusClient`, following the V1.71/V1.72 patch-route OCC convention:
  - `POST /v1/local/world-kb/patch_entity` — entity title, body, aliases, and block_type edits on `kb_key_blocks`.
  - `POST /v1/local/world-kb/promote_candidate` — adopt/reject/merge promotion state machine on `kb_extract_jobs`.
- **Per-row OCC conflict detection** (LOCKED): reuses existing columns — `kb_key_blocks.revision` (from `20260525_kb_key_blocks.sql`) + `kb_extract_jobs.version` (from `202606190001_kb_extract_jobs_and_pool_version.sql`) — no new migration needed; no world-level revision counter. Stale writes return **409 `WorldKbConflictError`** with entity locator + current revision; validation failures return **422 `WorldKbValidationError`**.
- **Backend prerequisite — promoted World KB Local API routes**: World KB read+write operations promoted from CLI-direct-DB (`creator world kb adopt/reject/edit/delete`) to **4 first-class Local API routes** with OCC revision tracking, making the World KB a normative Local API surface (not CLI-only).
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

V1.74 completes the World KB canvas surface by promoting first-class typed relationships from the V1.73 deferred slot into a shipped authoring surface. The relationship route is reachable from both the canvas graph and the complete non-spatial relationship view; both entry points call the same Local API contract and preserve the §5 `NexusClient` boundary.

> **Scope and roadmap SSOT**: [v1.74-world-kb-relationships-and-hygiene-compass-v1.md](../../iterations/v1.74-world-kb-relationships-and-hygiene-compass-v1.md) §0 grill decisions, §1.1 Track A, §1.3 wire contracts, and §2 normative specs. Architectural detail: [canvas-strategy-surface.md](canvas-strategy-surface.md) (V1.74 Shipped β), [entity-scope-model.md](entity-scope-model.md) §5.6, and [local-api-surface-conventions.md](local-api-surface-conventions.md) §7.6.

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

*Local-first Web UI product contract. V1.64 Shipped (Control Room + Setup) → V1.65 §13 Content-Authoring → V1.66 §14 Desktop Shell → V1.67 §15 Surface Convergence & De-risk → V1.69 Design System Maturation & Canvas Draft → V1.70 §16 Canvas Strategy Implement (α) + CI/desktop-build optimization → V1.71 §17 Canvas Strategy Write-Boundary (β) → V1.72 §18 Canvas Outline+Timeline (β) → V1.73 §19 Canvas World KB (β) → V1.74 §20 Canvas World KB Relationships (β). V1.75 roadmap: canvas-pivot candidate + 8 QC suggestions (`tbd-v1.75-qc-followup`). Design tokens: `apps/web/DESIGN.md` (V1.65 Standard+ + V1.66 desktop supplement + V1.69 Production migration + V1.70 canvas-token fill + V1.71 canvas-write tokens + V1.72 outline/timeline tokens + V1.73 canvas-worldkb tokens + V1.74 relationship tokens).*
