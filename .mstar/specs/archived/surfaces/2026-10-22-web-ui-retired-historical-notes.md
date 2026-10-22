# Local Web UI (Control Room + Setup → Content-Authoring) — Specification v1

**Status**: Shipped — local Web UI product contract: Control Room + Setup, content authoring, canvas surfaces (Strategy / Outline+Timeline / World KB / relationships), findings remediation, creator memory review loop, reading surface, SOUL visualization  
**Document class**: Feature line  
**Created**: 2026-06-24  
**Scope**: Nexus local Web SPA (`apps/web`), its `NexusClient` transport boundary and standalone TypeScript HTTP service (`apps/nexus-service`) under Electron lifecycle ownership; Control Room, Setup and content-authoring product stages; strict separation from the private cloud SaaS.

> **Host boundary:** Electron is the sole desktop host ([desktop-shell.md](desktop-shell.md)); the standalone `apps/web` SPA uses `apps/nexus-service` for `/v1/daemon/*` — that retained URL prefix does not imply a Rust daemon host.

**Coordinates with**:

- [cli-spec.md](../cli/cli-spec.md) — current CLI contract
- [rust-core-service-boundary.md](../architecture/rust-core-service-boundary.md) — current core / TS service boundary; [daemon-runtime.md](../../archived/runtime/2026-09-21-daemon-runtime.md) retains the retired axum/static-asset topology
- [schemas-external-consumer-boundary.md](../architecture/schemas-external-consumer-boundary.md) — the bundled UI is a first-class external consumer of `@42ch/nexus-contracts`
- [local-cloud-crate-architecture.md](../../archived/architecture/2026-09-21-local-cloud-crate-architecture.md) §1 (historical) — the original strict local-product vs cloud-product separation; current cohorts: [rust-core-service-boundary.md](../architecture/rust-core-service-boundary.md) §4.2
- Repo-root [`DESIGN.md`](../../../../DESIGN.md) + [`DESIGN.dark.md`](../../../../DESIGN.dark.md) — sole normative DESIGN pair (see §30)
- [daemon-api-surface-conventions.md](../runtime/daemon-api-surface-conventions.md) — cursor pagination / `ErrorResponse` / naming conventions the UI data layer relies on

---

## 0. Current architecture

`apps/web` is a standalone SPA, not a Rust-embedded asset feature. In the desktop product, Electron loads the built web assets through `nexus://app/index.html` and owns the local service lifecycle through `DesktopServiceController`. The standalone TS service opens the native core and serves the retained `/v1/daemon/*` HTTP surface. Browser development/preview runs the SPA separately with a proxy to that service.

Code anchors: [`apps/desktop-electron/src/main.ts`](../../../../apps/desktop-electron/src/main.ts) (web protocol, `DesktopServiceController`, standalone service entry), [`apps/nexus-service/src/index.ts`](../../../../apps/nexus-service/src/index.ts) (`startService`), [`apps/nexus-service/src/routes.ts`](../../../../apps/nexus-service/src/routes.ts), and [`apps/web/vite.config.ts`](../../../../apps/web/vite.config.ts) (dev/preview proxy).

---

## 1. Purpose

This spec defines the product contract for a type-safe, local-first Web UI:

- **Control Room** — read-heavy visibility into what the runtime is doing (Works, sessions, schedules, capabilities, findings).
- **Setup** — the write surface that configures the creative starting point (Work CRUD, preset management).
- **Content authoring** — chapter structure, outline (canvas), and the reading surface (§13, §18, §21, §25).

The Web UI makes the runtime legible and configurable to authors who are not terminal-fluent, without altering its data model or persistence.

---

## 2. Placement and product separation (normative)

### 2.1 Placement: OSS repo `apps/web`

The local Web UI lives in **this OSS repository** at `apps/web/` (a pnpm workspace member under `apps/*`), **not** in the private `nexus-platform` monorepo.

Rationale:

1. **Build coupling.** The SPA, Electron host, and TS service all build within this OSS repository, without the private repo's build graph.
2. **Type coupling.** The UI consumes `@42ch/nexus-contracts` via `workspace:*` so there is zero cross-repo version lag between wire schemas and the UI types. A private-repo placement would reintroduce npm-semver drift.
3. **Audience coupling.** This UI is a *local-first* surface for the local product line; it shares nothing with the cloud SaaS deployment model.

### 2.2 Strict separation from the private cloud SaaS

This is a **different product** from any web UI in the private `nexus-platform`:

| Dimension | Local Web UI (this spec, OSS) | Cloud SaaS (private `nexus-platform`) |
| --- | --- | --- |
| Deployment | standalone `apps/web` SPA; Electron packages the web assets and owns the local TS service lifecycle | hosted multi-tenant cloud |
| Data source | local `state.db` + reference store via loopback service API | platform HTTP / cloud DB |
| Audience | a single author on their own machine | platform tenants / cloud users |
| Auth | local service / desktop transport policy; §4.1 records the loopback model | platform auth / sessions |
| Roadmap home | this spec + `apps/web/` | `nexus-platform` `v1-spec/` |

**Invariant:** no cloud-product feature, platform auth flow, or platform-gated capability (DF-13/16/55/59; PD-05) is exposed in this UI while `platform_integration = paused`. The UI surfaces only the local product line. Cross-repo contract sharing is one-way: this repo's `schemas/` → `nexus-contracts`; the UI never imports platform-only types.

---

## 3. Stack (normative)

| Layer | Choice | Why |
| --- | --- | --- |
| Framework | **React 19** | largest ecosystem; mental-model consistency with the existing `@42ch/nexus-contracts` TS consumer surface |
| Build / dev server | **Vite** (SPA) | builds standalone web assets; dev/preview proxies the local TS service |
| Language | **TypeScript** (strict) | non-negotiable; end-to-end type safety via the codegen pipeline |
| Styling | **TailwindCSS** | utility-first, low design-debt, pairs with the component layer |
| Component primitives | **shadcn/ui** | copy-in components keep ownership inside the repo; no opaque runtime dependency |
| Server state | **TanStack Query** | matches the cursor-pagination + shared `ErrorResponse` retry model; mature |
| Client routing | **React Router** | standard SPA routing for the screen groups |
| Wire types | **`@42ch/nexus-contracts`** via `workspace:*` | zero version lag with `schemas/`; the UI is a first-class external consumer |

This stack is the **desktop-ready** foundation: it introduces no browser-only API in core logic, so the desktop shell wraps the same `apps/web/dist` without a frontend rewrite (see §5).

---

## 4. Serving and access model

The SPA and HTTP service are separate surfaces. Electron serves packaged web assets and owns `apps/nexus-service` startup/stop/handoff; the TS service, not `nexus42`, serves `/v1/daemon/*`. Dev/preview Vite proxies that prefix to the configured service endpoint. There is no integrated-daemon or `web-embed` release mode; see §0 for source anchors and [desktop-shell.md](desktop-shell.md) for lifecycle policy.

### 4.1 Auth model

The Web UI introduces **no new auth surface**. The HTTP service is keyless on loopback; the static shell needs no credential because it holds no data. The UI does not add login, sessions, or tokens. Any future remote (non-loopback) access is explicitly opt-in: the service requires its remote-bind flags plus TLS and an API key; the core Connect peer-tools gate is a separate boundary. Loopback remains the default.

> Implementation note for `daemon-api-surface-conventions.md`: the shared `ErrorResponse` (F-E1) is what the UI's toast/notification layer parses; the UI must never have to special-case per-handler error shapes.

---

## 5. `NexusClient` adapter boundary (normative)

Browser mode selects `BrowserClient`; desktop mode selects `DesktopClient` through the versioned preload bridge — [desktop-shell.md](desktop-shell.md) §5.

All daemon access from the UI goes through a single **`NexusClient`** interface. Core screen logic depends only on this interface, never on a concrete transport, so the same screens run unchanged in the browser and inside the desktop host.

```text
            ┌──────────────────────────────────────────┐
Control Room │  screen components → TanStack Query       │
 + Setup     │       → NexusClient (interface)           │
            └──────────────┬───────────────┬────────────┘
                           │               │
              BrowserClient│               │ DesktopClient (desktop bridge)
              fetch http://localhost:<port>│ invoke(...)
                           ▼               ▼
                   /v1/daemon/*  (local TS service)
```

- **`BrowserClient`** — `fetch` against `http://localhost:<port>/v1/daemon/*` (browser mode).
- **`DesktopClient`** — implements the same interface via the desktop bridge; a thin subclass of `BrowserClient` adding the desktop-only methods.

The interface exposes the Daemon API resources the MVP consumes (conceptual — exact signatures are owned by the client layer):

| Resource group | Operations | MVP screen |
| --- | --- | --- |
| Works | list (cursor), get, create, patch, archive | Works dashboard; Work CRUD |
| Orchestration sessions | list, get (status) | Sessions view |
| Schedules | list (per Work), get | Schedule/cron view |
| Capabilities | list | Capability registry browser |
| Findings | list (per Work) | Findings view |
| Presets | list, get, create, update, delete, **validate** | Preset management CRUD |

**Invariant:** screens must not call `fetch`/`invoke` directly; they call `NexusClient`. This keeps the desktop host a one-impl change rather than a rewrite.

---

## 6. MVP surface (Control Room + Setup)

Seven screen groups. READ = visibility; CRUD = write/setup.

### 6.1 Control Room (READ-heavy — visibility)

| # | Screen group | Purpose | Mode |
| --- | --- | --- | --- |
| 1 | **Works dashboard** | List Works (cursor-paginated) with status + completion %; drill into a Work detail view (intake status, stage, world binding, linked schedules). | READ |
| 2 | **Orchestration sessions** | List sessions with per-session status (running / completed / failed); watch what the runtime is doing right now. | READ |
| 3 | **Schedule / cron** | List schedule identities. Cron declaration editing lives in the CLI (`creator works cron`); the UI is read-only here. | READ |
| 4 | **Capability registry browser** | List `nexus.*` capabilities with descriptions — surfaces the agent tool bridge so authors can see what the runtime can do. | READ |
| 5 | **Findings** | List findings (per Work, F-P2 endpoint) with status / severity filtering. Remediation actions are §23. | READ |

### 6.2 Setup (writes — configure the starting point)

| # | Screen group | Purpose | Mode |
| --- | --- | --- | --- |
| 6 | **Work CRUD** | Create / patch (status, stage) / archive a Work. Foundational for any authoring journey; mirrors `creator works` CLI. | CRUD |
| 7 | **Preset management** | Full CRUD on presets — list / show / create / update / delete / **validate** (dry-run). | CRUD |

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

## 8. Non-goals

- **Remote (non-loopback) access / any new auth** — out of scope; would require its own auth spec.
- **Mobile (native v2 mobile targets)** — deferred. **Durable roadmap:** DR-61.
- **OpenAPI spec generation + generated TS client SDK (C2)** — deferred; TanStack Query + codegen TS types suffice for the SPA.
- **Cloud-product features** — see the §2.2 invariant while `platform_integration = paused`.

---

## 10. Wire contracts note

This Feature line adds **no new wire schemas** of its own. It consumes the daemon-api schemas and their conventions:

- Depends on **F-E1** (shared `ErrorResponse`) for unified UI error handling.
- Depends on **F-P1** (Works cursor pagination) for the dashboard list.
- Depends on **F-P2** (findings list endpoint) for the findings view.

Versioning, npm/Rust bumps, and list-shape changes are owned by [daemon-api-surface-conventions.md](../runtime/daemon-api-surface-conventions.md).

---

## 13. Content-Authoring UI

The browser SPA carries an authoring surface layered on the Control Room + Setup screens. All screens route through the same `NexusClient` interface (§5) and consume the chapter-content Daemon API (conventions in [daemon-api-surface-conventions.md](../runtime/daemon-api-surface-conventions.md)).

- **Chapter structure table** (per-Work, multi-Work switcher reusing the Works dashboard entry): columns — chapter #, title (**display-only** — derived from outline frontmatter or slug/chapter# fallback; no `title` column exists in `work_chapters`), slug, planned word count, volume, status (`not_started` / `outlined` / `draft` / `finalized` / `published`), actual word count. Sortable by chapter #.
- **Structure CRUD**: edit slug / planned word count / volume; advance status `not_started → outlined` (reverse transitions gated). (Title is display-only — no DB column; title authoring happens in the outline editor.) `finalized` / `published` chapters are protected: structural edits require a confirmation dialog; **deletion is hard-blocked**.
- **Outline authoring on the canvas**: outline editing is a canvas surface (§21) — the chapter inspector edits outline prose via the `content` field on `outline.patch_chapter`.
- **Body read-only rendering**: render a chapter's `body_path` markdown (frontmatter-aware — surface status/metadata in a read-only header strip, render body prose read-only). Right-click context menu offers **"Copy path"** only (browser clipboard write; path sourced from the API).
- **Soft concurrency**: no hard lock. Editing the outline of a chapter already in `draft` or `finalized` status shows a non-blocking but unmissable persistent banner. The banner states plainly: editing the outline will **not** re-draft the body, and guides the author to the explicit next step — reverse-transition the chapter status to `outlined` (then advance to `draft`) via structure-CRUD to trigger a re-draft. Orchestration reads the outline at draft-time (a natural snapshot of whatever is on disk).

### 13.1 The authoring loop this enables

1. **Plan** — shape a chapter's outline (on the canvas, §21); the outline is the author-facing planning document that orchestration reads to draft body prose.
2. **Review** — read a chapter's rendered body read-only; copy its file path to open it in the author's own editor.
3. **Restructure** — fix slugs, volumes, planned word counts; advance a chapter from `not_started` to `outlined` once its outline is ready. (Title text is shaped in the outline editor, where the chapter heading naturally lives.)

**The runtime owns body drafting.** Body prose is written by the orchestration engine through the host-tool bridge; the UI has no body write path (see §13.2). The UI is the *planning and structure* surface; the runtime remains the *drafting* surface.

### 13.2 Non-goals

- **Body full-text editor (`body_path` write)** — **rejected**. Nexus is an AI-autonomous executor — the AI owns prose; a manual rich-text body editor is the wrong direction. (It would also require a per-chapter edit-lock design, MD↔rich-text lossless round-trip, frontmatter/status sync, and a conflict policy with the orchestration co-writer.)
- **"Open with" / "Reveal in file manager"** — desktop-host capabilities (native `shell.open` / `openWith` with a scope whitelist), owned by [desktop-shell.md](desktop-shell.md) §4; not a web-surface capability. The browser keeps "Copy path" only.
- **Drag-to-reorder chapters / bulk chapter operations / manual reconcile trigger / outline template library** — deferred.

### 13.3 User stories

- **Structure CRUD** — *As an author*, I can fix a chapter's slug, planned word count, and volume, and advance its status from `not_started` to `outlined`, so the structure of my Work reflects my plan.
- **Protected edits** — *As an author*, when I edit the structure of a `finalized` or `published` chapter the UI asks me to confirm, and it refuses to delete one, so I cannot accidentally destroy settled work.
- **Body read + copy path** — *As an author*, I can read a chapter's rendered body and copy its file path, so I can open it in my own editor to read or annotate.
- **Soft-concurrency awareness** — *As an author*, when I edit the outline of a chapter that is already drafted, the UI tells me plainly that my change will not re-draft the body and shows me the explicit next step (reverse the chapter status to `outlined` to trigger a re-draft), so I am not left waiting or surprised.
- **Multi-Work navigation** — *As an author*, I can switch between my Works while planning, so I can keep several projects in flight from one window.

### 13.4 Wire contracts

The authoring surface consumes the chapter-content schemas (chapter list with cursor + `items`, chapter detail, body GET read-only, `work_profile` on Work requests, full preset CRUD routes) and the outline/timeline patch routes (§18). Versioning, npm/Rust bumps, and per-DTO `schema_version` increments are owned by [daemon-api-surface-conventions.md](../runtime/daemon-api-surface-conventions.md).

---

## 15. Surface Convergence

The Daemon API surface uses one error envelope, one array-key convention, and casing discipline; the Create-Work dialog carries a work-profile selector; the TS client exposes preset CRUD methods.

### 15.1 What ships

- **Work-profile selector in the Create-Work dialog**: creating a new Work includes a profile selector offering `novel`, `essay`, `game-bible`, and `script`. The wire contract carries `work_profile` as an additive optional field; skipping the selector yields the default (`novel`). The canvas tailors the steering surface per profile.
- **Preset CRUD on the TS client**: `GET/PATCH/DELETE /v1/daemon/presets/{id}` are exposed on the `NexusClient` interface. There is **no form-based preset-management UI** — the canvas Strategy surface owns preset editing (§16–§17).
- **`items` array-key convention**: all schema-backed list responses use `items` (`works`/`schedules`/`sessions`/`capabilities` in the legacy shape). The UI data layer adapts internally; the author experiences nothing.

### 15.2 User story

- **Work-profile selector** — *As an author*, when I create a new Work I can choose its profile (novel, essay, game-bible, or script) from a selector in the Create-Work dialog, so the runtime and the canvas can tailor the steering experience to the kind of thing I am writing.

(Preset-management stories live on the canvas Strategy surface (§16–§17), where preset editing is a graph operation, not a form.)

---

## 16. Canvas Strategy surface (α)

The **Strategy (Preset) surface** is the human-steerable Canvas surface at α depth: read + visualization + live overlay + Idea-steer. The human *sees* the Strategy as a graph and *steers* execution with an Idea; the AI owns prose. Architectural detail: [canvas-strategy-surface.md](canvas-strategy-surface.md).

### 16.1 What ships

- **Shared Canvas Shell** (`@xyflow/react`, route-split behind `/strategy`): React Flow provider, pan/zoom, minimap, dot-grid, selection model, side inspector, validation panel (read-only), keyboard shortcuts, screen-reader graph summary, `NexusClient` transport injection. **Route-split verified**: `strategy-page-*.js` is a separate 305 kB chunk; React Flow is excluded from the Control Room bootstrap.
- **Strategy graph adapter (read projection)**: preset YAML → React Flow `nodes`/`edges` per [canvas-strategy-surface.md](canvas-strategy-surface.md) §3.2 — outer state-machine states → top-level nodes; `inner_graph` states → group/sub-flow nodes (`parentId` + `extent:parent`); Converge merge-point states → join nodes (`wait_for_all` / `first_completed` / `any`); linear `next` / labeled `branches` / default → edges with condition labels. **10 unit tests** in `strategy-graph.test.ts` cover the mapping.
- **Live execution overlay (bounded)**: session `current_task_id` + `status` → node highlight + status ring, polled at 5 s. **Bounded to session-level** — completed-path history + child-session hierarchy are deferred (**Durable roadmap:** DR-29).
- **Idea-input affordance + Run/Resume/Steer**: persistent canvas affordance for Idea input (global entry + contextual node action). Submitting an Idea enqueues/resumes via **existing** `addSchedule` / `editCoreContext` + `signalSchedule` (promoted onto `NexusClient`). Verbs: **Steer / Run / Resume / Ask Nexus to revise**. Idea submissions land as visible steering artifacts.
- **Canvas → DESIGN token fill**: the 11 canvas tokens (`canvas-surface`, `canvas-grid`, `canvas-node-fill/-hover/-border/-border-selected`, `canvas-edge/-hover`, `canvas-port`, `canvas-minimap`, `canvas-strategy-accent`) carry concrete light + dark values in the repo-root DESIGN pair. Token *names* are preserved. `canvas-strategy-accent` derives from the purple family.
- **Accessibility baseline**: keyboard-focusable nodes/edges, non-spatial alternate view (Strategy states in execution-order list + transition table), ARIA live-region graph summary, focus-visible rings, read-only inspector. These a11y requirements ([canvas-strategy-surface.md](canvas-strategy-surface.md) §4.4) are product requirements, not just tech checkboxes.

### 16.2 The steering loop this enables

1. **Read the Strategy as a graph** — *As an author*, I see my Strategy (preset) rendered as a state-machine graph with visible join/wait nodes, so I understand how Nexus will execute my Work before it runs.
2. **Steer by Idea** — *As an author*, I express an Idea (Work-level or on a specific node) and choose **Steer / Run / Resume / Ask Nexus to revise**, then Nexus executes — drafting prose, advancing the chapter, updating the KB — so I direct the work without typing the body myself.
3. **Review AI execution on the canvas** — *As an author*, after Nexus executes, I see what changed on the canvas (node status, generated-output links, pending instructions) and review the result read-only, so I stay in command of an autonomous process.

(The outline+timeline and World KB surfaces, plus structured node-granular *edits* of the Strategy graph, are §17–§20.)

### 16.3 Wire contracts

**`wire_contracts_changed: FALSE`** — no schemas/codegen/`@42ch/nexus-contracts`/DTO change. The Idea-steer path reuses existing schedule input/core-context/signal surfaces. `GET /v1/daemon/presets/{id}` returns `GetPresetResponse { id, source, path?, yaml }` — sufficient for client-side Strategy graph projection; `GET /v1/daemon/orchestration/sessions/{session_id}` returns `SessionDetailResponse { session: SessionSummary }`, bounding the live overlay to current-node/status highlighting plus session-level state. Rich overlay data (completed-path history, child-session hierarchy) is deferred (DR-29) rather than adding a read route.

### 16.4 Non-goals

- **CLI / schema rename of `preset` → `strategy`** — deferred; the internal CLI/schema identifier rename remains deferred (architect LOCKED: internal identifiers unchanged, poor ROI).
- **Desktop distribution v2** (signing / tri-OS / auto-update) — deferred. **Durable roadmap:** DR-58.
- **Rich live overlay** (completed-path history, child-session hierarchy) — deferred. **Durable roadmap:** DR-29.

### 16.5 User stories

- **Read the Strategy as a graph** — *As an author*, I see my Strategy (preset) rendered as a state-machine graph with visible join/wait nodes, so I understand how Nexus will execute my Work before it runs — and I can rewire a branch or adjust a gate on the canvas *(editing is §17; this surface is read + overlay + Idea-steer)*.
- **Steer by Idea** — *As an author*, I express an Idea (Work-level or on a specific node) and choose **Steer / Run / Resume / Ask Nexus to revise**, then Nexus executes — drafting prose, advancing the chapter, updating the KB — so I direct the work without typing the body myself.

(Outline chapters, World KB entities, and non-Strategy graph editing live in §18–§20 stories.)

---

## 17. Canvas Strategy Write-Boundary (β)

The Strategy canvas is **editable at node granularity** while preserving the core boundary: the browser webview never writes raw files. All Strategy edits flow through schema-backed Daemon API patch routes, daemon validation, atomic persistence, and graphRevision conflict handling. Architectural detail: [canvas-strategy-surface.md](canvas-strategy-surface.md) and [daemon-api-surface-conventions.md](../runtime/daemon-api-surface-conventions.md) §7 patch-route pattern.

### 17.1 What ships

- **Strategy patch routes**: the UI calls three Daemon API routes through `NexusClient`, not `fetch`/native filesystem access:
  - `POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/patch` (`StrategyPatchStateRequest` → `StrategyPatchResponse`) for state label/description edits.
  - `POST /v1/daemon/strategies/{strategy_id}/transitions/patch` (`StrategyPatchTransitionRequest` → `StrategyPatchResponse`) for edge/transition condition and target rewiring.
  - `POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/prompt/patch` (`StrategyPatchPromptTemplateRequest` → `StrategyPatchResponse`) for in-node prompt-template edits.
- **Conflict modal copy/flow**: stale writes return 409 `StrategyConflictError` with the current revision and structured locator. The canvas keeps the user's draft patch, refetches the canonical Strategy, and presents a modal with:
  - **Headline**: "This node changed while you were editing."
  - **Body**: "Nexus updated **{node label}** to revision **{current_revision}** while you were editing **{field}**. Your change is still in the inspector."
  - **What changed**: the canonical value that now differs from the user's last known revision.
  - **What you were about to do**: the user's draft value for the same path.
  - **Actions**: **Use current** (primary/default), **Reapply my edit**, and **Review side-by-side** (enabled only when draft and canonical changes touch non-overlapping fields; disabled for same-field/path or prompt-template conflicts). Cancel returns focus to the originating inspector.
- **Canvas inspector copy**: state inspector header "Edit state" with fields "Label" and "Description"; edge inspector header "Edit transition" with fields "Condition" and "Target state"; prompt-template node header "Edit prompt"; inline validation errors and a save-in-progress indicator; a 409 surfaces the conflict modal instead of a generic error.
- **graphRevision tracking + freshness indicator**: the client tracks `lastKnownRevision` per Strategy; the service stores the canonical revision as `revision:` in the preset YAML header. Existing presets without the key read as revision `0`; the first accepted patch writes `revision: 1`. The canvas chrome or command palette shows "Strategy · revision {revision} · updated {relative time}" with "Refresh now"; when a newer revision exists it shows "Strategy updated elsewhere · revision {newer} available · Refresh to see latest".
- **Canvas-write DESIGN tokens**: concrete light/dark tokens for write-mode states (selected/focus border, save-in-progress, conflict marker), preserving the token-name invariant.

### 17.2 The editing loop this enables

1. **Inspect** — the author selects a Strategy state, edge, or prompt-template node and edits only that structured node/subresource.
2. **Validate** — the service verifies ids, reachability, condition syntax, prompt-template references, and revision preconditions before accepting the patch.
3. **Commit or resolve** — successful patches return a new revision and canonical state; conflicts present current-vs-draft recovery instead of silently overwriting newer orchestration changes.

### 17.3 Non-goals

- **No CLI/schema rename of `preset` → `strategy`** — UI terminology remains Strategy, persisted identifiers and routes continue to expose `preset` where already shipped.

---

## 18. Canvas Outline+Timeline (β)

The canvas extends to the **Outline+Timeline surface** — the second of the three canvas surfaces — bringing Work-structure (Volume → Chapter → Scene/Beat) and timeline events onto the graph with structured patch routes, outlineRevision conflict handling, and non-spatial alternate views.

### 18.1 What ships

- **3 outline/timeline patch routes**: the UI calls three Daemon API routes through `NexusClient`, following the Strategy patch-route convention (§17):
  - `POST /v1/daemon/works/{work_id}/outline/patch` for volume-level structure edits (order, title, metadata).
  - `POST /v1/daemon/works/{work_id}/chapters/{n}/patch` for per-chapter edits (slug, planned word count, volume assignment, status advancement).
  - `POST /v1/daemon/works/{work_id}/timeline/patch` for timeline event CRUD (timestamp, description, linked chapters).
- **outlineRevision conflict policy**: stale writes return 409 with outlineRevision locator; conflict modal adapts the Strategy pattern (§17.1) with `{node_label}` placeholder substituting chapter/event/volume domain entity names.
- **Outline-flavored conflict modal UX**: same structural flow as Strategy but with domain-appropriate copy ("This chapter changed…" / "This event changed…") and outline-flavored actions (Use current, Reapply my edit, Review side-by-side).
- **Non-spatial alternate views**: sortable chapter list + sortable timeline event list (with virtualization), satisfying the accessibility requirement for non-spatial navigation.
- **8 outline/timeline canvas-write DESIGN tokens**: `canvas-outline-volume-fill` + 4 chapter-card status tokens + `canvas-outline-timeline-event-pin` + `canvas-outline-foreshadow-edge` + `canvas-outline-timeline-marker` + `canvas-outline-conflict-marker` — concrete light + dark values; token names preserved verbatim.
- **Atomic outline markdown persistence**: outline body preserved under `RuntimeLockGuard`; never overwritten by canvas writes.
- **Body ownership invariant**: outline markdown body remains editor-owned and is never overwritten by canvas writes.

### 18.2 The planning loop this enables

1. **Inspect and edit structure** — the author opens the Outline canvas, sees chapter nodes organized by volume, edits chapter properties (slug, word count, status) inline on the node or via the inspector.
2. **Timeline alongside outline** — the author positions timeline events relative to chapters, seeing both on the same canvas with timeline edges connecting to corresponding chapter nodes.
3. **Resolve conflicts** — when an orchestration session or another author changes the outline concurrently, the conflict modal surfaces the delta with chapter/event labels and recovery actions.

### 18.3 Non-goals

- **No in-canvas markdown body editing** — outline body remains editor-owned; canvas writes only structure fields.

---

## 19. Canvas World KB (β)

The World KB canvas surfaces the author's accumulated world knowledge (characters, locations, items, events, organizations, conflicts, and computable derived entities) as a graph with entity nodes, promotion-state lifecycle badges, source-anchor provenance edges, and structured patch operations. World KB read+write operations are first-class Daemon API routes with per-row OCC revision tracking (not CLI-only).

### 19.1 What ships

- **2 structured patch routes**: the UI calls two Daemon API routes through `NexusClient`, following the Strategy patch-route OCC convention (§17):
  - `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` — entity title, body, aliases, and block_type edits on `kb_key_blocks`.
  - `POST /v1/daemon/worlds/{world_id}/kb/promote-candidate` — adopt/reject/merge promotion state machine on `kb_extract_jobs`.
- **Per-row OCC conflict detection**: reuses existing columns — `kb_key_blocks.revision` (from `20260525_kb_key_blocks.sql`) + `kb_extract_jobs.version` (from `202606190001_kb_extract_jobs_and_pool_version.sql`) — no world-level revision counter. Stale writes return **409 `WorldKbConflictError`** with entity locator + current revision; validation failures return **422 `WorldKbValidationError`**.
- **World KB Daemon API routes**: World KB read+write operations are first-class Daemon API routes with OCC revision tracking, making the World KB a normative Daemon API surface (not CLI-only).
- **Canvas UI — World KB graph projection**: entity nodes (all block_type variants per [entity-scope-model.md](../architecture/entity-scope-model.md) §5.1.1, plus computable blocks) + promotion-state lifecycle badges (pending → confirmed/rejected/merged, with `manual` state) + source-anchor provenance edges + computable badges (age, reference count, confidence). Route-split behind the shared Canvas Shell.
- **Canvas UI — entity inspector + promotion inspector**: entity inspector shows title, body, aliases, block_type, revision, and source anchors; promotion inspector shows candidate metadata (source, confidence, timestamp), current state, and promotion action buttons. Both inspectors surface inline validation errors and a save-in-progress indicator.
- **Canvas UI — KB-flavored conflict modal**: same structural pattern as Strategy (§17.1) and Outline (§18.1) conflict modals, but with World-KB-appropriate copy ("This entity changed while you were editing." / "This candidate's state changed while you were editing.") and KB-flavored actions (Use current, Reapply my edit, Review side-by-side). Cancel returns focus to the originating inspector.
- **Non-spatial alternate view**: sortable entity list (name, block_type, promotion state, last modified) with virtualization — satisfies the accessibility requirement for non-spatial navigation.
- **17 `canvas-worldkb-*` DESIGN tokens**: concrete light and dark values in the repo-root DESIGN pair for World KB node fills/borders/selection, promotion-state badges (confirmed/rejected/merged/pending), source-anchor edges, computable badges, conflict markers, and entity-inspector chrome — token names preserved verbatim.

### 19.2 The knowledge loop this enables

1. **See the world as a graph** — the author opens the World KB canvas and sees all entities (characters, locations, items, etc.) laid out as a connected knowledge graph with source-anchor edges showing provenance — so the author understands the web of world knowledge the AI has accumulated and can trace every fact back to its source.
2. **Edit entity details** — the author selects an entity node, edits its title, body, aliases, or block_type via the inspector, and commits with OCC protection — so the author refines the AI-extracted world without overwriting concurrent extractions.
3. **Curate extracted knowledge** — the author opens the promotion inspector for a candidate fact, sees its source, confidence, and extraction context, then adopts, rejects, or merges it — so the author is the final curator of what goes into the canon.
4. **Resolve conflicts** — when the AI extracts a newer version of a fact while the author is editing, the KB-flavored conflict modal surfaces the delta and recovery actions — so the author never silently loses changes.

### 19.3 Non-goals

- **No canvas-pivot retirement of the KB CLI surface** — the KB CLI stays normative.

---

## 20. Canvas World KB Relationships (β)

First-class typed relationships between World KB entities: the relationship route is reachable from both the canvas graph and the complete non-spatial relationship view; both entry points call the same Daemon API contract and preserve the §5 `NexusClient` boundary.

### 20.1 What ships

- **Relationship patch route**: `POST /v1/daemon/worlds/{world_id}/kb/patch-relationship` (`WorldKbPatchRelationshipRequest` → `WorldKbPatchRelationshipResponse`) supports `add`, `update`, and `remove` with `expected_version` OCC on `kb_relationships.revision`. `GET /v1/daemon/worlds/{world_id}/kb/graph` populates `relationships[]`; symmetric rows emit derived reverse projections read-side without duplicate storage rows.
- **Canvas relationship-edge rendering**: graph edges display relationship type labels, confidence badges, and grounding badges. Symmetric reverse projections share the same storage `relationship_id` as the stored direction.
- **Relationship inspector**: create/edit/delete UI exposes source and target entity pickers, `WorldKbRelationshipKind` taxonomy dropdown, `custom_label` field for `custom`, `symmetric` toggle, optional anchor multi-picker, and display-only confidence. Inline validation surfaces self-loop, taxonomy, anchor, and confidence errors.
- **Three creation entry points**: drag from an entity handle, right-click an entity and choose “Connect to…”, or select **New Relationship** from the non-spatial relationship table. All entry points use the same quick-create flow and then the full relationship inspector.
- **Conflict modal reuse**: stale writes reuse the KB-flavored conflict modal pattern with relationship copy, current-vs-draft diff, and actions **Use current**, **Reapply my edit**, and **Cancel**.
- **Non-spatial relationship table**: the relationship table is a complete accessible write-surface with create/edit/delete parity, sortable columns, keyboard reachability, and accessible action labels.

### 20.2 The relationship loop this enables

1. **Connect entities** — the author creates a typed edge between two World KB entities from the canvas or table without leaving the UI.
2. **Qualify meaning** — the author chooses a core relationship kind or `custom_label`, marks symmetry when appropriate, and optionally attaches source anchors.
3. **Resolve conflicts** — concurrent service/canvas relationship edits return 409 before mutation; the conflict modal keeps the draft and lets the author refresh or reapply from the current version.

### 20.3 Non-goals

- **No relationship taxonomy management UI** — the core enum ships in contracts; authors use `custom` + `custom_label` for out-of-enum meanings.

---

## 21. Canvas-Pivot — outline authoring on the canvas

The whole-document TipTap outline editor (the `chapter-page.tsx` Outline tab + `usePutChapterOutline` PUT save path) is retired. The node-granular Outline canvas (§18) is the **sole outline authoring surface**. The canvas chapter inspector provides outline-prose TipTap editing via the `content` field on `outline.patch_chapter` (parity-close — the inspector replicates the retired editor's rich-text capability: headings, bold, italic, lists, markdown round-trip via `tiptap-markdown`).

`chapter-page.tsx` is a read-only body view with an "Edit outline → Canvas" redirect CTA — the reading/preview value is preserved (body prose render, frontmatter metadata strip, Copy Path), and outline authoring is relocated (not lost) to the canvas. This is a pre-1.0 hard cutover: no dual-editor deprecation period; the pivot is a clean retire+replace. The reading view is promoted to a designed reading surface in §25.

---

## 22. World KB relationship auto-extraction and confidence

**Extraction proposes relationships**: `nexus.llm.extract` emits relationship candidates (entity pairs + relation_type + confidence + source anchors) from chapter text, persisted behind a `needs_review=1` gate (`source='extraction'`); the canvas shows them in a **Suggested pane** (sortable by confidence, default high→low, per-row Promote/Delete + bulk Promote all).

**Confidence-weighted UX**: graph edges render with stepped confidence bands (low <0.4: 1px/30%, mid 0.4–<0.7: 2px/60%, high ≥0.7: 3px/100%) consuming the DESIGN `canvas-worldkb-relationship-confidence-*` tokens; confidence-band colored badges (red/amber/green, uniform 8px) appear on edge labels. No new DESIGN colors — the stepped bands reuse the shipped confidence tokens.

**`needs_review` gate semantics**: extraction defaults to `needs_review=1`; GET graph defaults to excluding suggested rows (`?include_suggested=true` surfaces them); suggested edges render dashed (distinct from confirmed solid).

**Curation**: promotion clears `needs_review` via the existing `world_kb.patch_relationship` route (`needs_review: false` on update); `source` stays read-only provenance.

---

## 23. Findings-Remediation UI

The Control-Room findings page is a full **remediation authoring surface** that closes the "observe → triage → resolve" quality loop in the UI, exactly as the canvas closed the "steer → execute → review" writing loop. The backend ships the full findings PATCH surface (6-state lifecycle adjacency enforcement, 7-field `UpdateFindingRequest` payload, full CRUD routes, stale-count endpoint); the web app consumes them with no new backend routes. Lifecycle detail: [findings-lifecycle.md](../contracts/findings-lifecycle.md). API surface: [daemon-api-surface-conventions.md](../runtime/daemon-api-surface-conventions.md) (findings PATCH reference).

### 23.1 What ships

The findings page gains three remediation affordances consuming the shipped `PATCH /v1/daemon/works/{work_id}/findings/{finding_id}` route (7-field `UpdateFindingRequest` payload, all fields optional), plus a detail-panel layout, TanStack Query mutations, and DESIGN triage tokens. No new backend routes — the PATCH surface already exists (D2 LOCKED: types are already generated and barrel-exported; no codegen config change needed).

- **Status transitions** — inline status dropdown or action buttons driving the 6-state findings lifecycle: `open` → `triaged` → `in_review` → (`resolved` | `wont_fix` | `duplicate`). Invalid transitions are disabled client-side per the server-enforced adjacency (DAO `is_valid_transition()` table; illegal transitions return HTTP 422 `INVALID_TRANSITION`). Status change persists via PATCH, the findings list refetches, and the stale-findings count in the daemon status bar updates on mutation invalidation.

- **`target_executor` assignment** — dropdown/selector routing the finding to `brainstorm`, `write`, `master`, or `none`. `target_executor` is an *assignment* (route hint for triage), not an auto-trigger — re-running an orchestration session stays a deliberate canvas/CLI action. Valid values per `crates/nexus-local-db/src/findings.rs:192`.

- **Inline edit** — finding detail/inspector panel exposes the 7 `UpdateFindingRequest` fields (all optional): `title`, `description`, `severity`, `kind`, `rule_suggestion`, `status`, `target_executor`. Edits persist via PATCH with optimistic TanStack Query mutations (`useUpdateFinding`); the findings list + stale-count queries refetch on mutation invalidation. The inspector shows full finding context: `chapter`, `source_schedule_id`, `routing_hint`, `kind`, timestamps.

- **Detail-panel + row-action hybrid layout** (D4 LOCKED) — the findings page remains a Control-Room table (not a canvas graph); a detail/inspector panel with the three remediation affordances supplements row-level status/severity badges, reusing existing `Table` + `StatusBadge` components. Row-level actions (status dropdown, assignment selector) enable quick triage without opening the inspector. A11y: the canvas non-spatial-alternate-view discipline applies to canvas graph surfaces, not to Control-Room tables.

- **TanStack Query mutations with optimistic updates** — `useUpdateFinding` updates locally before the server responds, rolls back on error, and invalidates the findings list + stale-count queries on success. In the last-writer-wins model (D1b — no OCC, no revision column, no conflict modal; the quality loop is single-author-triage), optimistic updates are safe with no conflict modal needed.

- **DESIGN tokens** — findings status badges (6 states: `open`, `triaged`, `in_review`, `resolved`, `wont_fix`, `duplicate`), severity colors (reusing existing `severityVariant` where possible), triage chrome (action buttons, assignment selector, inline-edit affordance). Token names preserved verbatim.

- **Codegen wiring** — `UpdateFindingRequest` and `CreateFindingRequest` are generated and barrel-exported by `@42ch/nexus-contracts` (D2 LOCKED: codegen is glob-based, both types exist on disk at `packages/nexus-contracts/src/generated/daemon-api/findings/` and `crates/nexus-contracts/src/generated/daemon_api/findings/`). The web app imports them in `apps/web/src/lib/nexus/types.ts` and wires `getFinding`/`updateFinding` onto the `NexusClient` interface + `BrowserClient`/`DesktopClient` implementations. No codegen config change, no schema change.

### 23.2 The triage loop this enables

The UI closes the **observe → triage → resolve** quality loop without dropping to the CLI:

- **Triage status** — *As an author*, I can review a finding, assess its severity and context, and advance its status through the 6-state lifecycle (`open` → `triaged` → `in_review` → `resolved` / `wont_fix` / `duplicate`) directly from the findings page, so the quality loop progresses without CLI commands.

- **Route by assignment** — *As an author*, I can assign a finding's `target_executor` to `brainstorm`, `write`, `master`, or `none` — routing it to the appropriate stage of the writing loop — so my triage decisions are visible and actionable when I return to the canvas or CLI to re-run orchestration.

- **Edit finding details inline** — *As an author*, I can correct a finding's title, description, severity, kind, or rule_suggestion in the inspector panel without dropping to the terminal, so I can refine quality metadata as I triage.

- **Cross-profile triage** — *As an author*, I can triage findings for any Work (novel, essay, game-bible, or script) from the same findings page — the DB and Daemon API are already profile-agnostic, so no profile-specific UI restriction is introduced.

### 23.3 Non-goals

- **One-click orchestration re-trigger from a finding** — **rejected**. Re-running a brainstorm/write session stays a deliberate canvas/CLI action. The canvas is the intended steering surface for re-runs; a finding "Re-run" button would couple UI remediation to scheduler semantics and overlap the canvas steering surface. `target_executor` is an *assignment* (route hint), not an auto-trigger.

- **Findings producer changes** — the findings page consumes findings the quality loop already produces. Changing *what* findings are produced (new rules, new review-master output, new extraction) is out of scope.

- **New quality-loop rules / rubrics** — the 五问 rubric and review-master presets are unchanged.

- **New canvas surfaces** — this is a Control-Room surface, not a canvas surface.

- **Body editor** — **rejected** direction (Nexus is AI-autonomous executor; AI owns prose).

- **Platform publish** — platform paused (local-only).

- **Mobile** — future scope.

---

## 24. Creator Memory Review-Loop UI

The Control-Room Memory page lets an author review the pending captures their sessions produced, summarize them into long-term memory, and browse the resulting fragments — all from the web app, without dropping to the terminal. The memory Daemon API is contracted in `schemas/daemon-api/memory/`; handler DTOs are generated types. `createPendingReview` stays CLI/producer-only — the session-end capture pipeline owns creation; the UI is review/consume-only, exactly mirroring the `createFinding` CLI-only decision.

### 24.1 What ships

The Memory page (`apps/web/src/pages/memory-page.tsx`) gains three affordances consuming the shipped memory Daemon API, all creator-scoped (`creator_id` on every endpoint; the service rejects a mismatched creator with 403). No new backend routes.

- **Pending-review list with count badge + delete** — a Control-Room table (cursor-paginated, default 50, max 250) driven by `GET /v1/daemon/memory/pending-review?creator_id={id}`. The header shows a live count badge from `GET …/pending-review/count?creator_id={id}` (`CountPendingReviewsResponse { count }`, polled). Each row shows a humanized `task_kind` chip, truncated `session_id`, truncated `raw_digest`, and `created_at` in local time. A per-row delete action calls `DELETE …/pending-review/{pending_id}?creator_id={id}` with a web-friendly confirmation; the row disappears optimistically and the count decrements.

- **"Review & Summarize" action** — a prominent primary CTA (enabled only when `count > 0`) triggering `POST /v1/daemon/memory/review` (`ReviewRequest { creator_id }`). The UI shows a processing state during the server-side pipeline (the passthrough classifier promotes/fragments/drops each pending row), then surfaces `ReviewResponse { promoted, fragmented, dropped }` in a confirmation toast (e.g., "Review complete — 3 promoted to long-term memory, 5 saved as fragments, 2 dropped"). On success, the pending-review list, count, and fragments queries all refresh.

- **Fragments browser** — a read-only list from `GET /v1/daemon/memory/fragments?creator_id={id}&keyword={opt}&limit={n}` (default 50, optional case-insensitive keyword filter). Each row shows `fragment_id` (monospace) and `summary`. Fragments are produced **only** by the `review` route — no manual CRUD exists on this surface.

- **Inspector panel** — selecting a pending-review row opens a side inspector (matching the findings detail-panel layout) showing all 6 `PendingReviewInfo` fields: `pending_id` (monospace badge), `session_id`, `world_id` (or "(none)" if absent), `task_kind` (humanized chip), `raw_digest` (scrollable preformatted area), `created_at` (RFC 3339 → author's local time). Clicking the selected row again or the panel's delete affordance dismisses it.

- **`NexusClient` promotion** — five methods added to the interface (`listPendingReviews`, `countPendingReviews`, `deletePendingReview`, `reviewMemory`, `listMemoryFragments`) and implemented on `BrowserClient` (inherited unchanged by `DesktopClient`). Adapter-contract parity guard extended. `createPendingReview` is intentionally absent (CLI/producer-only).

- **TanStack Query mutations with optimistic updates** — `useDeletePendingReview` removes the row and decrements the count before the server responds, rolls back on error, and invalidates pending-list + count + fragments on settle. `useReviewMemory` surfaces the result counters in a toast and invalidates the same three query sets on success. The pending-review list uses `useInfiniteQuery` for cursor pagination (reuses the `LoadMore` component).

- **`creator_id` UI source** — the active creator id is derived from the most recent session/schedule (`useActiveCreatorId`), mirroring the canvas's `useDerivedCreatorId` derivation (`apps/web/src/lib/canvas/use-strategy-data.ts`). The model is single-active-creator (config.toml); sessions/schedules are themselves creator-scoped, so their `creator_id` is the active creator. A first-class active-creator endpoint/context is a future surface.

- **DESIGN tokens** — 13 memory tokens (`memory-pending-count`, `memory-review-button`, `memory-fragment-summary`, `memory-fragment-id`, 5 `memory-task-kind-*` chips, `memory-inspector-header`/`-field-label`/`-field-value`, `memory-fragment-filter-input`). Token names preserved verbatim.

### 24.2 The review loop this enables

The UI closes the **capture → review → internalize** self-loop with direct visibility into the memory pipeline:

- **Review pending captures** — *As an author*, I can see every pending capture my sessions produced (with a live count badge), inspect its full context (session, world, task kind, raw digest, timestamp), and delete a capture I do not want internalized — so the memory pipeline does not silently accumulate noise.

- **Summarize into long-term memory** — *As an author*, I can trigger Review & Summarize to run the rule-based classifier over my pending queue, and see exactly how many captures were promoted to long-term memory, saved as fragments, or dropped — so I understand what my memory actually contains.

- **Browse long-term fragments** — *As an author*, I can browse my long-term memory fragments (optionally filtered by keyword) and read their summaries — so I can recall what the system has internalized about my creative work without dropping to the CLI.

### 24.3 Non-goals

- **`createPendingReview` from the UI** — **rejected** (D-UX LOCKED). The session-end capture pipeline owns pending-review creation; the UI is review/consume-only. Mirrors the `createFinding` CLI-only decision.

- **Fragment CRUD** — **rejected** (D-UX LOCKED). Fragments are produced only by the `review` route (the sole fragment producer). No manual fragment create/edit/delete exists in the backend and none is introduced.

- **LLM-backed summarizer** — **out of scope**. The `review` route runs the shipped passthrough summarizer (`UNTRUSTED` provenance, 256 KiB truncation); the UI documents shipped behavior and must not introduce a new LLM-backed summarizer or change review rules.

- **Active-creator context provider** — the current derivation (sessions/schedules) is sufficient for the single-active-creator model. A first-class active-creator endpoint/context is a future surface.

- **New canvas surfaces** — this is a Control-Room surface, not a canvas surface.

- **Mobile** — future scope.

---

## 25. Author Reflection: Reading Surface + SOUL Visualization

Two independent UI tracks under the shared theme **Author Reflection**: a manuscript reading surface with in-context maturation indicators (Track A) and a SOUL personality visualization over internalized memory fragments (Track B).

### 25.1 What ships

**Track A — Manuscript reading surface (P0)**

- **Designed reading experience**: the `chapter-page.tsx` reading view (read-only body render + frontmatter strip + Copy Path + "Edit outline → Canvas" redirect) is a designed reading surface with legible reading typography (light + dark), chapter/volume navigation (prev/next, keyboard ←/→), and persisted reading progress (§28). No new write routes — body-ownership invariant preserved (canvas remains the sole authoring surface).
- **In-context lightweight maturation indicators**: three indicators visible on the reading surface without navigation — chapter completion-state badge (from `work_chapters.status`), World KB density count (from `kb_key_blocks` count), and open-findings count (from `findings` non-terminal lifecycle rows). Read-only consumption of existing data.
- **Profile scope**: novel-first; other profiles (essay/game-bible/script) render read-only-prose-compatible via the same body render. Profile-specific reading chrome deferred. `wire_contracts_changed: FALSE` for Track A.

**Track B — SOUL personality visualization (P1)**

- **Keyword clusters**: frequency/cluster visualization of the creator's internalized memory fragment `keywords`, scoped per creator (`creator_id` from the fragments table). Surfaces the top accumulated themes — what the author and their AI assistants have been focusing on.
- **Temporal drift timeline**: `created_at`-axis showing fragment accumulation + keyword composition shift over time, with fragment count folded into the timeline. Answers "how has my creative focus shifted?" — the core reflection insight.
- **Sparse-data graceful degradation**: three states (empty → low-data → rich) with empathetic, encouraging copy at each state. New creators see a forward-looking empty state, not a broken chart.
- **Memory page integration**: both viz surfaces integrate into the Control-Room Memory page as a new tab/section — no new top-level route.
- **Wire additive**: `memory-fragment-info` carries optional `keywords` (array) + `created_at` (string, RFC 3339).

### 25.2 The reflection loops this enables

**Track A — review-augmented reading**:

1. **Read as a reader** — the author opens any chapter in a comfortable, distraction-free reading view with book-like typography, rather than raw markdown.
2. **See maturity at a glance** — without leaving the reading surface, the author sees the chapter's completion state, World KB richness, and open findings count — making the reading surface actionable for review.
3. **Navigate the manuscript** — chapter/volume navigation with session progress lets the author flow through their manuscript naturally.

**Track B — seeing who you are becoming**:

1. **Discover creative themes** — the author sees keyword clusters surfacing what their creative work has internalized into SOUL memory.
2. **Track focus over time** — the temporal drift timeline shows how the author's themes have shifted — "am I drifting toward or away from the writer I want to be?"
3. **Start from nothing with confidence** — new creators see an encouraging empty state that explains the feature's value proposition and what actions will populate it.

### 25.3 Non-goals

- **Standalone maturation dashboard** (multi-chart cross-Work/World aggregate) — deferred. **Durable roadmap:** DR-30.
- **Independent growth-curve view** as a separate SOUL visualization — folded into temporal drift; standalone deferred.
- **Persisted reading progress + annotations/highlights** — shipped (§28).
- **Per-World SOUL filtering** — deferred. **Durable roadmap:** DR-32.
- **Profile-specific reading chrome** — deferred. **Durable roadmap:** DR-31.
- **DF-49 (Standalone MCP server)** — **cancelled** (not deferred), conflicts with ACP-client product direction and creates circular-invocation risk.
- **Any new write route on the reading surface** — read-only consumption only.

---

## 26. Creator SOUL Maturation: Narrative, Projection, Growth, Refresh

Four additions deepen the reflection axis, anchored on a definitive product model:

> **Creator SOUL is the creator's core creative identity — world-agnostic. It is the *whole*: all accumulated `memory_fragments`. A per-World SOUL projection is the Creator SOUL's inclination within a specific world — a *subset* of fragments filtered by the world they emerged from. It is a drill-down view, not a separate identity. The LLM personality narrative synthesizes the whole into a reflective "who you are becoming" statement and is world-agnostic by definition; the world projection only filters the read-side viz (keyword clusters, temporal drift, growth-curve).**

This product model is user-visible in the UI — the world selector explicitly frames a world projection as "a subset of your Creator SOUL," not a separate identity.

### 26.1 What ships (four spec points)

**SP-1 — Creator-SOUL Narrative (headline)**

- **Narrative card** (`soul-narrative-card.tsx`): a surface above the existing keyword/drift viz on the SOUL tab. The card synthesizes an LLM-generated reflective narrative — "who you are becoming" as a creative identity — from the author's accumulated fragment themes, temporal shifts, and preoccupations. The narrative is generated on-demand ("Reflect on my SOUL") and persisted (cached); it is not regenerated on every page load.
- **Five UX states (each testable)**:
  1. `ungenerated` — CTA card with a "Reflect on my SOUL" button; preview hint text below explaining what the narrative will show.
  2. `generating` — loading skeleton with pulse; button shows spinner + "Reflecting…"; no timeout panic.
  3. `current` — prose block with `generated_at` timestamp; "Re-reflect" secondary action.
  4. `stale` — cached narrative still visible; banner above: "You've grown since this reflection — new fragments have arrived." with "Re-reflect" CTA on the banner.
  5. `insufficient-data` — empty-state illustration: "Your SOUL is still forming. Keep writing and reviewing — once you've accumulated enough creative experience, Nexus can reflect on who you are becoming." Below, a fragment count with "X more to go" when close to the threshold.
- **Narrative quality threshold** (product-gated, architect-enforced in the prompt contract): a narrative is "good enough" when it (1) references at least two distinct theme keywords from the creator's fragment clusters, (2) references at least one shift or development over time, and (3) ends with a forward-looking reflection or question. Below the minimum-fragment threshold, the endpoint returns the insufficient-data state rather than risking a thin/generic narrative.
- **Consumes**: `POST /v1/daemon/memory/soul/reflect` via a `useSoulNarrative` query/mutation. The request carries required `creator_id` and optional `force_regenerate`; the response exposes `state`, cached/generated narrative fields when present, stale snapshots, current counts, and insufficient-data thresholds.

**SP-2 — World Projection Selector**

- **World selector**: a dropdown/list control in the SOUL section header, defaulting to "All worlds" (the whole Creator SOUL). Selecting a specific world re-scopes the keyword clusters, temporal drift, and growth-curve to that world's fragment subset. It does **not** re-scope the LLM narrative or its generation metadata; the narrative remains Creator-level / world-agnostic.
- **Subset semantics (UX contract)**:
  - Worlds with fragments list their name + fragment count (e.g., "Eryndor (42 fragments)"); worlds with Works but no fragments may appear with the subset-empty state.
  - Worlds with zero fragments and zero Works are **omitted** from the selector — no dead-end empty options.
  - "All worlds" is always the default; the label clarifies "your whole Creator SOUL."
  - If a selected Work-backed world has no fragments yet, the viz area shows the subset-empty copy: "No fragments in this world yet — your Creator SOUL is still shaped by your work here when fragments arrive."
- Drives a `world_id` query param on the fragments query; coordinates with the page-level `fragmentKeyword` lift pattern in `memory-page.tsx`.

**SP-3 — Growth-Curve (BL-10)**

- **Growth-curve component** (`growth-curve.tsx`): cumulative fragment count over time as a simple line/area chart, independent of the temporal-drift timeline (which answers "how has my focus shifted?" — the growth-curve answers "how much have I accumulated?").
- **Three density states** (reuses the `densityFor` branching):
  - `empty` (0 fragments): forward-looking illustration + "Your SOUL begins here — every review session adds a fragment to your creative growth."
  - `low-data` (1–9 fragments): simple chart + "Your SOUL is taking shape. Keep writing to see your growth curve emerge."
  - `rich` (≥10 fragments): full cumulative curve with axis labels and a summary stat.
- Respects the world projection (re-scopes when a world is selected).

**SP-4 — Auto-Refresh**

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

### 26.3 Non-goals

- **Narrative editing / curation by the author** — deferred. **Durable roadmap:** DR-33 (SOUL narrative curation/editing + export/share).
- **Narrative export / share** — deferred. **Durable roadmap:** DR-33 (SOUL narrative curation/editing + export/share).
- **Async background-job infrastructure** — on-demand generation only.
- **BL-09 standalone maturation dashboard** — backlog.
- **Profile-specific reading chrome** — deferred (**Durable roadmap:** DR-31); persisted reading progress + annotations/highlights shipped (§28).
- **Realtime websocket / push** — poll + invalidation only.
- **Rewrite of existing keyword/drift viz** — the world filter is surgical; no restructuring beyond adding the selector.

---

## 27. SOUL Completion: per-World Narrative + Titled Worlds Selector

Each world gets its own narrative — the Creator SOUL's inclination within that world — and the selector is wired to real world titles from the existing endpoint.

> **Creator SOUL is the creator's core creative identity — world-agnostic. It is the *whole*. A per-World narrative is the Creator SOUL's inclination within a specific world — a *subset* of that whole. Each has its own narrative. "All worlds" shows the Creator-level narrative. Selecting a world shows that world's own per-World narrative.**

This model is locked in the UI. The world selector shows titles (not ids) and drives both visualization scope and narrative scope. A world may have insufficient data for its own narrative even when the Creator whole does not.

### 27.1 What ships (two spec points)

**SP-1 — per-World Narrative (headline)**

- The narrative card re-renders when the world selector changes.
  - "All worlds" → the Creator-level narrative (states unchanged).
  - A specific world → that world's per-World narrative, generated on-demand and invalidated per (creator, world).
- Per-world narrative states are independent of the Creator-level narrative. A world may show `insufficient_data`, `ungenerated`, or `stale` even when the Creator-level narrative is `current`. The card surfaces the world-specific state; it does not fall back to or restate the Creator-level narrative.
- **Distinct + valuable gate** (product threshold): a per-World narrative is shown only when it (a) surfaces at least one theme or temporal shift that is specific to that world's fragments and not prominent in the Creator-level narrative, or (b) meets the same three-point quality bar (≥2 theme keywords + ≥1 shift + forward-looking close) scoped to the world's own fragments. A thin restatement of the Creator-level narrative that adds no world signal fails the gate and surfaces the per-world insufficient-data state instead.
- The per-World narrative uses the same five UX states as the Creator-level narrative (ungenerated / generating / current / stale / insufficient-data), with world-specific copy and thresholds.
- Selector-driven: "Reflect on this world's SOUL" (or equivalent clear label) triggers generation for the selected world.

**SP-2 — Titled world selector (worlds-endpoint wiring)**

- The world selector consumes the existing `GET /v1/daemon/narrative/worlds` (already returns `title`; workspace-scoped under the single-creator local model). No new backend endpoint. Ownership of a selected world is enforced server-side at `/soul/reflect` via `narrative_worlds.owner_creator_id` — the selector itself does not perform ownership filtering.
- Renders world **titles** (not raw ids). "All worlds" remains the default and explicitly labels "your whole Creator SOUL."
- Includes Work-backed worlds (those with Works but no fragments yet). These appear with an honest subset-empty state: "No fragments in this world yet — your Creator SOUL is still shaped by your work here when fragments arrive."
- Worlds with zero fragments and zero Works remain omitted (no dead-end options).
- Selector change is the single source of truth: it re-scopes the keyword clusters, temporal drift, growth-curve, *and* the narrative card. No stale narrative from a prior selection remains visible after a change.

### 27.2 The reflection loops this completes

1. **See the whole** — default "All worlds" shows the complete Creator creative identity (Creator-level narrative).
2. **Drill into a world** — selecting a world shows how the author's themes manifest within that world's context as a distinct per-World narrative, not just filtered viz.
3. **Honest per-world states** — a world with little data shows its own insufficient state; the UI never pretends a thin per-World narrative is the Creator-level narrative.
4. **Titled, reachable worlds** — authors choose by name; Work-backed worlds are visible even before fragments arrive.

**Distinct vs thin product gate** (enforced at the surface):

- Worlds that have accumulated enough specific signal show a valuable per-World narrative.
- Worlds that have not yet produced world-specific signal show the graceful insufficient state rather than a thin restatement of the Creator-level narrative.

### 27.3 Non-goals

- Narrative comparison across worlds (side-by-side or diff view) — out of scope.
- Narrative editing or curation by the author — read-only LLM output; only "re-reflect." **Durable roadmap:** DR-33 (SOUL narrative curation/editing + export/share).
- Narrative export or share paths. **Durable roadmap:** DR-33 (SOUL narrative curation/editing + export/share).
- Realtime websocket / push (poll + invalidation only).
- A new backend worlds endpoint (existing one is wired).
- Rewrite of the SOUL components beyond the per-World extension (surgical).

---

## 28. Deeper Manuscript Reading

**Status**: Shipped — persisted reading progress + character-offset annotations/highlights with drift notice. Profile-specific reading chrome deferred.

### 28.1 Author-visible outcomes

- Reopening a chapter restores the author's last scroll position automatically (across reloads, tabs, and the desktop shell).
- Selecting text in the reading surface creates a persistent highlight with an optional note. The highlight survives navigation away from the chapter and reappears when the author returns.

### 28.2 Scope

Persisted reading progress is tracked per (creator, work, chapter).

Annotations and highlights are anchored by character offsets into the current body plain text of the chapter. The color enum is limited to `{yellow, blue, green, pink}`. Each highlight may carry an optional free-text note.

### 28.3 Read-only body invariant

The reading surface does not mutate `body_path` or outline files. Canvas remains the sole authoring surface per the canvas pivot (§21). Any future "accept annotation into manuscript" flow must go through the canvas / Outline API, not the reading surface.

### 28.4 Drift notice

When a highlight's stored offsets no longer fit the current body text (after a body edit performed in the canvas), the UI must show a clear, non-blocking notice instead of mis-rendering the highlight or silently failing.

### 28.5 Non-goals

- Profile-specific reading chrome (essay section breaks, game-bible cross-reference overlays, novel typography presets, etc.).
  > **Durable roadmap:** DR-31 (profile-specific reading chrome).
- Standalone maturation dashboard.
  > **Durable roadmap:** DR-30 (standalone maturation dashboard).
- Body or outline editing from the reading surface.
- Annotation range reconciliation or fingerprinting across body edits.
- Rich-text or threaded annotations on highlights.
- Real-time sync or cloud features.

---

## 29. Information Architecture

### 29.1 Purpose

The sidebar is a two-tab information architecture (Creator | Orchestrator) with nested nav, footer profile switcher, and simplified daemon status bar.

**Entrance axis (AR-15):** the two-tab structure is a **separate axis** from the [Entrance](../../../../CONCEPTS.md#entrance) (User-layer `developer` | `content-creator`). The sidebar renders entrance-filtered `navGroups` from `ENTRANCE_DESCRIPTORS[entrance]`; the Creator|Orchestrator tabs and `tabFromPathname` are untouched. **Create** (content-creator) is a *reduced* tree that hides the EL §3 operator chrome (agent catalog, preset manager, sessions, schedule, rules authoring, inspector, Connect, capability browser); **Develop** (developer) is the full Control Room plus the Develop hub v1 land route. `_system.*` presets stay hidden in both trees.

### 29.2 Sidebar — two-tab structure

The sidebar renders at `lg`+ (≥961px) as a fixed left panel. Below `lg`, the two-tab structure collapses to a top dropdown or pill scroller.

**Two top-level tabs** (composited as horizontal tab bar at the top of the sidebar):

| Tab | Top-level items | Nested children |
|-----|----------------|-----------------|
| **Creator** | Works | (per-Work navigation nest: Chapters, Findings) |
| | Creator | Memory, SOUL |
| **Orchestrator** | Runtime | Sessions, Schedule, Capabilities |
| | Harness *(user-visible label; route `/strategies` unchanged — PD-4)* | (single entry: `/strategies` list → `/strategies/:presetId` canvas detail) |

**Excluded from sidebar**:
- **Connect** — Settings → **Connection** (`/settings/connection`); legacy `/connect` permanently redirects (C1). Not a sidebar item.
- **Daemon status** — leaves sidebar; lives in the status bar (running = restart-icon only) and the main-banner (degraded/error).

There is no flat 10-item `NAV_ITEMS` array; tab switch swaps the visible nav items, and the footer Profiles row is always visible regardless of active tab.

**Entrance-filtered nav (AR-15):** the sidebar's nav groups come from `ENTRANCE_DESCRIPTORS[entrance].navGroups` (typed `ShellNavGroup[]` in `entrance-registry.ts`), not a single static tree. The footer also gains a **Switch entrance** control (not a third top-level tab) that remounts the matching layout tree without resetting Creator, workspace, or `setup_completed`. Create hides the EL §3 hide-table surfaces from nav, settings, and in-tree links (soft-remove — pages are not deleted; direct navigation bounces to the Create land route `/works` with the `entrance.bounceToast` one-shot toast, except `allowDeepLink` surfaces such as the strategy canvas). Develop lands on `/developer` (Develop hub v1) and reaches every surface.

### 29.3 Nested nav behaviour

- Top-level items expand/collapse nested children.
- Per-Work nested nav (Chapters, Findings) is contextual to the currently active Work.
- Creator nested nav (Memory, SOUL) is static per the creator scope.
- Runtime nested nav (Sessions, Schedule, Capabilities) is static.
- Harness is a single top-level entry under Orchestrator — no nesting; clicking opens `/strategies` list. **User-visible label = "Harness"** (PD-4 rename); the route path `/strategies` and `CanvasSurfaceKind = 'strategy'` are internal identifiers that stay unchanged.

### 29.4 Strategies unification (routes) — user-visible label = "Harness"

The `/presets` (list) and `/strategy` (canvas) routes collapse to:

- **`/strategies`** — list view (replaces `presets-page.tsx` as entry).
- **`/strategies/:presetId`** — canvas detail view (preserves the canvas surface verbatim).

**PD-4 rename:** the user-visible pillar-entry label is **Harness**. The route paths (`/strategies`, `/strategies/:presetId`), the `CanvasSurfaceKind = 'strategy'` enum value, the `preset` wire/DTO field names, CSS classes, and all internal identifiers stay **unchanged** — this is a user-visible-copy-only rename. **Preset stays** as the mechanism name under Harness (it is not a pillar synonym).

**Preservation invariant**: The full canvas surface is preserved verbatim at `/strategies/:presetId` — React Flow behaviour, write-boundary (strategy patch routes, graphRevision), conflict modals, non-spatial alternate views, keyboard paths, and all canvas-write DESIGN tokens. This is an entry-point reshape only; no canvas rewrite.

**Redirect compatibility**: `/presets` → `/strategies` (301/302); `/strategy` → `/strategies/:presetId` (requires a stored active preset ID or redirects to list). Deep links must resolve.

### 29.5 Footer profile switcher

Rendered at the bottom of the sidebar (always visible regardless of active tab). Slack/Chrome-style horizontal row of Creator avatar icons.

**Interaction contract**:
- **Click / Enter/Space**: switches `active_creator_id`; dependent queries refetch.
- **Keyboard**: arrow-left/right to navigate avatars; Home/End for first/last; Esc closes any transient UI (modal, dropdown).
- **"+" CTA**: opens a lightweight create-Creator modal consuming the existing `POST /v1/daemon/creators` endpoint.
- **Single-Creator case**: exactly one avatar + "+". Clicking the single avatar is a no-op (no error toast). The "+" is the only call-to-action.
- **Persistence**: `active_creator_id` stored in `localStorage` (key: `nexus:activeCreatorId`) for browser; in desktop mode the switch also goes through the `switch_active_creator` bridge operation, which persists it to the main-owned config (desktop-shell.md §8). Restored on reload.
- **Avatar fallback**: initials (first character of `display_name`) or generic icon when no image; must be accessible (not color-only).

### 29.6 Daemon status bar simplification

There is no 5-state status pill + always-enabled Start button. The simplified contract:

- **Running/healthy**: status bar shows **only** a restart-icon button (no pill, no state text, no Start button).
- **Degraded/error/stopped/crash**: a **top-of-main-content banner** (`main-banner.tsx`) surfaces the failure with error detail + Restart CTA. Not a sidebar item.
- **Never**: enabled-while-broken Start button; silent hang during service startup.

The daemon status bar is desktop-only in the current build; it subscribes to the preload bridge's `onDaemonStatusChanged` event and re-syncs periodically through `getDaemonStatus()` (desktop-shell.md §4).

### 29.7 Button contrast invariant

Recorded in repo-root [`DESIGN.md`](../../../../DESIGN.md) §Component Primitives/Button and [`DESIGN.dark.md`](../../../../DESIGN.dark.md):

> **Every button (or button-like element) with a dark, primary, or saturated background MUST use light/white text in both light and dark themes.**

`DESIGN.dark.md` frontmatter `components.button.primary.textColor` is `#ffffff` (dark primary uses `dark:bg-brand-cyan dark:text-white`, matching hover/active).

A sweep audit of all button call sites in `apps/web/src/**` enforces the dark-bg → light-text invariant. Vitest snapshot coverage (light + dark themes) gates regressions.

### 29.8 Browser-build contract

The wizard and per-launch daemon-ready gate are **desktop-first**:
- **Desktop**: `setup_completed` read through the versioned preload bridge operation `get_setup_completed()`; wizard renders when false **after** the fullscreen Daemon gate reaches Ready (see §29.13).
- **Browser**: defaults `setup_completed = true` (i.e. no wizard). The daemon-ready gate is a no-op or instant pass. No desktop-bridge calls are assumed in the browser build. The browser SPA must not regress — existing Vite dev / static-serve flows continue unchanged.

### 29.9 Non-goals

- Full multi-creator CRUD / profile management (rename, delete, avatar upload) — footer switcher only selects existing or creates via existing endpoint.
  > **Durable roadmap:** DR-34 (multi-creator CRUD / multi-workspace UI / workspace switcher).
- Multi-workspace UI / workspace switcher.
  > **Durable roadmap:** DR-34 (multi-creator CRUD / multi-workspace UI / workspace switcher).
- Agent detection during non-first-launch.
- Mobile full rewrite (the `<lg` collapse preserves the two-tab structure as a dropdown/pill scroller but is not optimized for touch-first patterns).

### 29.12 First-launch hardening invariants

- The workspace-location row must be flex-safe at desktop window sizes: content containers that hold the resolved path can shrink, and the path truncates inside the card instead of expanding the right edge.
- The native Browse path uses the existing desktop capability/IPC boundary. The frontend sends `defaultPath` to the `pick_directory` bridge operation and does not introduce a second argument shape or compatibility shim. If the picker cannot open, the user sees a readable toast/error, not a raw missing-key failure.
- The daemon step observes existing desktop daemon-status state and `detail` only. It must not require new daemon API fields, generated schemas, or contract package changes.
- Clean-state smoke and existing-install smoke are hard product verification gates, not new UI features. Their evidence may be captured manually or with automation, but unit tests alone do not prove the author-visible first-launch path.

### 29.13 First-launch wizard reshape (Entrance-first + app-level Daemon gate)

**Product behavior (author-visible).** The wizard separates daemon readiness from wizard steps and reduces the wizard to author-facing choices; the Entrance step is first (Entrance → Agent → Workspace → Done).

#### 29.13.1 App-level fullscreen Daemon gate (not a wizard step)

- **Every desktop launch** — first-launch and return visits — shows a fullscreen Daemon wait until Ready before `/setup` or main UI.
- **Outer gate:** `DaemonLaunchGate` wraps `AppRoutes` in `apps/web/src/App.tsx`; renders `DaemonReadySplash` until Ready on desktop.
- **Inner gate:** `SetupGate` on main-shell routes only — after Ready, `setup_completed=false` → `/setup`; `true` → main UI. Splash logic lives outside `SetupGate`.
- Desktop **always** starts or attaches its managed service on app open (D2; desktop-shell.md §7.1/§13.10.1) — independent of `setup_completed`.
- The **Daemon wizard step does not exist**. Diagnostic UX (timeout, retry, stderr detail, reset-local-database recovery) lives on the splash/gate surface — not a numbered setup step.
- Happy path does **not** use `startDaemon` IPC from the wizard; recovery only on splash error paths.

#### 29.13.2 Four-step wizard IA (Entrance → Agent → Workspace → Done)

| Step | Step ID | Author-facing label | Module |
|------|---------|---------------------|--------|
| 1 | `entrance` | Entrance | `setup-step-entrance.tsx` (AR-17) |
| 2 | `agent` | Agent | `setup-step-agent.tsx` |
| 3 | `workspace` | Workspace | `setup-step-workspace.tsx` |
| 4 | `done` | Done | `setup-step-done.tsx` |

Orchestrator: `setup-wizard-page.tsx` — `WizardStep = 'entrance' | 'agent' | 'workspace' | 'done'`; initial step `entrance`.

**Entrance first (AR-17):** the Entrance step is first — the choice is cheap and daemon-independent, so the remaining steps can frame themselves for the chosen layout, and the first gated render after `finish()` already sees the chosen tree. `WizardState.entrance` (init `content-creator`) is persisted via `setEntrance` **before** `markCompleted()` so post-wizard navigation lands in the right layout. Desktop-only (browser `setup_completed` is always true; the browser path uses the first-run identity page instead).

**Removed:** Welcome step (`setup-step-welcome.tsx`); Daemon step (`setup-step-daemon.tsx`).

Agent scan remains `POST /v1/daemon/agent-host/scan` via `useScanAgents` after gate Ready (grill-me **B**). Scan-safety constraints per [desktop-shell.md](desktop-shell.md) §14 (bounded concurrency; no shell expansion). No desktop-side PATH probe duplicate.

Bootstrap (`ensureSetupBootstrap`) on Workspace **Continue** only.

#### 29.13.3 Portrait wizard shell

- Fixed portrait card: **`480px`** max width (`--color-setup-wizard-wizard-max-width`), **`min(720px, 85vh)`** height (`wizard-max-height: 720px` + `max-h-[85vh]`); content scrolls inside the card (`flex-1 overflow-y-auto` on step body).
- Top horizontal `TopStepIndicator` (Entrance / Agent / Workspace / Done); **no** left `w-setup-wizard-surface-step-panel-width` (208px) rail.
- Studio visual SSOT: `apps/design-studio/src/fixtures/setup-wizard-chrome-fixtures.tsx` before App wiring.
- Centered-card patterns (toast errors, bottom CTA, Browse adjacency) are preserved where applicable — only layout chrome differs.

#### 29.13.4 Settings Re-run Setup (R1 compatibility)

- Settings → Setup → **Re-run Setup** semantics unchanged: confirm clears `setup_completed` marker only; workspace path and agent profile **not** deleted.
- After confirm, author passes the fullscreen gate, then enters the four-step wizard (Entrance-first).

#### 29.13.5 Browser-build contract (unchanged)

§29.8 browser defaults remain: wizard and daemon gate are desktop-first; browser build skips wizard and gate.

#### 29.13.6 Non-goals

- Native PATH agent scan; multi-workspace switcher; BYOK; Settings shell IA redesign; wire/schema changes unless P0 proves unavoidable.
  > **Durable roadmap:** DR-35 (PATH agent scan), DR-34 (multi-workspace switcher + Settings shell IA redesign). BYOK: tracked as **DF-70**.

### 29.14 Studio-first pipeline + first-launch polish

**Implement authority** for Stretch Settings IA may defer.

#### 29.14.1 TopStepIndicator SSOT

- **Single module:** `apps/web/src/components/setup/top-step-indicator.tsx` exports `WizardStep` and `TopStepIndicator`.
- **Studio:** imports `@web-setup/top-step-indicator` — no duplicate inline implementation in fixtures.

#### 29.14.2 AgentPicker density

- Shared `apps/web/src/components/setup/agent-picker.tsx` accepts optional `density?: 'default' | 'compact'` (default `'default'`).
- Settings omits prop; wizard may pass `compact` only. No second picker module.

#### 29.14.3 Contract boundary

Prefer `wire_contracts_changed: false`. The studio-first invariant applies to all author-facing chrome.

#### 29.14.4 Voice & Content — first-launch + daemon chrome

Normative copy lives in repo-root [`DESIGN.md`](../../../../DESIGN.md) (`### Launch & daemon status`, `### Done step copy`, `### States`). Examples:

| Surface | Example copy |
|---------|----------------|
| DaemonReadySplash (waiting) | **Starting daemon…** — *This takes a few seconds on first launch.* |
| DaemonReadySplash (error) | **Daemon not ready** — **Restart Nexus** |
| Done step | **You're ready 🎉** — *Open Nexus to start writing. You can change settings anytime.* — **Open Nexus** |
| AgentPicker loading | *Scanning for local ACP agents…* |
| AgentPicker empty | **No agents found on PATH** |
| Settings nav (Stretch) | **Agent** · **Workspace** · **Advanced** |

### 29.15 Studio UI tune + coverage hygiene

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

Toast is promoted to `@42ch/nexus-ui` for Studio fixtures; App `apps/web/src/lib/use-toast.tsx` is a **thin re-export** — closes `R-V1106P0-001`. Do not claim App/package Toast parity until FB-012 lands.

#### 29.15.5 Contract boundary

Prefer `wire_contracts_changed: false`.

### 29.16 Frontend i18n

| Plan | Spec | Scope |
| --- | --- | --- |
| P0 — i18n foundation + Appearance | i18n foundation | `i18next` + `react-i18next`; Settings **Appearance** → Language (`system` \| `en` \| `zh-CN`); caller-owned copy; user-facing-only catalogs |
| P1 — full user-facing UI migration | i18n UI migration | Remaining **user-facing** `apps/web` chrome + `format.ts` / `Intl` active-locale wiring |

Settings nav is **Agent · Workspace · Appearance · Advanced**; the theme toggle stays in the header (not moved into Appearance). Developer-auxiliary surfaces (including `apps/design-studio`) remain out of catalog scope. `wire_contracts_changed: false`.

### 29.17 Creation peer groups + Canvas-first work shell

#### 29.17.2 Canvas-first work shell (`/works/:workId/*`)

- **Layout:** `WorkShellLayout` nested in `RootLayout` `<main>` — center canvas outlet + right `WorkRail` (280px at `lg`+).
- **Default:** `/works/:workId` → `/works/:workId/outline`.
- **Rail MVP:** Works list + metadata preview (`WorkSummary`); no manuscript snippet.
- **Responsive:** `<lg` → end-sheet drawer for rail.
- **Retired:** the drill-in pattern (`isDrillIn` / `drillInItems`) as primary enter-work UX; Creator \| Orchestrator tabs stay visible inside work.

#### 29.17.3 Contract boundary

`wire_contracts_changed: false`. Studio-first for new shell chrome per repo-root `DESIGN.md`.

#### 29.17.4 Worlds-first Creator sidebar

**Status:** Draft — supersedes the earlier Creator tab list-mode peer groups only (not §29.18 Canvas World-entry Timeline default).

| Creator tab group | Contents | Route |
| --- | --- | --- |
| **Worlds** (primary) | Worlds list | `/worlds` |
| **Works** | Works list | `/works` |

**Removed from Creator sidebar:** Timeline peer group, Work Timelines group, Memories (→ Orchestrator first group per P1). `/timeline` deep links and canvas Timeline surfaces remain per §29.16.

### 29.18 Three-pillar pivot + Timeline-first Canvas IA

#### 29.18.1 Canvas IA — World entry default flips to Timeline

The **World-entry** default is Timeline — an author meets a World's history first, not its entity graph. **Work entry is unchanged.**

**Current route clarification (2026-09-29):** [`apps/web/src/App.tsx`](../../../../apps/web/src/App.tsx) explicitly nests an index `<Navigate to="timeline" replace />` under `worlds/:worldId`, alongside the `timeline` route rendering `TimelinePage`. Thus `/worlds/:worldId` redirects to `/worlds/:worldId/timeline`; the latter is also directly navigable. There is no World-detail-page index route. The Work Timeline remains a sibling of `outline`, not the Work index.

| Entry context | Route | Default surface | Prior default |
|---|---|---|---|
| **World entry** | `/worlds/:worldId` → **Timeline** (index redirect / `/timeline`); Worlds list pick-target updates | **Timeline (World-building hero)** | `/worlds/:worldId/kb` (World KB) |
| **Work entry** | `/works/:workId` → Outline | **Outline (Timeline-companion)** — unchanged (§29.17.2) | Outline |

The Canvas shell hosts **four** peer surfaces: Strategy / Outline (Timeline-companion) / **Timeline** / World KB. The `CanvasSurfaceKind` enum carries `"timeline"` as an additive peer value. The Outline (Timeline-companion) surface keeps its chapter-relative timeline lane (Work projection); Timeline (World-building hero) projects the World KB graph's `block_type=event` entities onto a when-axis. Surface contract: [`canvas-strategy-surface.md`](canvas-strategy-surface.md) §3.3.2 + §4.5.

#### 29.18.2 Pillar framing

The Web UI is the primary home of the **Canvas** pillar (spatial steering surface, with Timeline-centric World building as the hero). The **Harness** pillar (orchestration/agent host/capability registry) user-visible label is **Harness** (PD-4); **Preset stays** as the mechanism name; internal identifiers remain unchanged. The **Computable** pillar (WASM reactivity) is user-visible through **Run Studio** on the Modules surface and **compute-on-Timeline** entry with Compute result nodes (direct lane routes: [daemon-api-surface-conventions.md](../runtime/daemon-api-surface-conventions.md) §12.3). Pillar definitions: [`STRATEGY.md`](../../../../STRATEGY.md) + [`CONCEPTS.md`](../../../../CONCEPTS.md).

#### 29.18.3 Contract boundary

`wire_contracts_changed: false`. The Timeline surface is additive frontend only (new `CanvasSurfaceKind = "timeline"` enum value + new Timeline adapter module under `apps/web/src/components/canvas/timeline-canvas/`); no `schemas/`, no codegen, no service Rust change, no `@42ch/nexus-contracts` version bump. Timeline reads `WorldKbGraphResponse` (§19) and writes through `kb.patch_entity` (§19) only — full architect-locked contract in [canvas-strategy-surface.md](canvas-strategy-surface.md).

### 30. Design Studio dev surface (not author-facing)

**Product classification.** `apps/design-studio` is a **contributor/dev auxiliary app** — a read-only gallery for the unified DESIGN SSOT, brand VI, and `apps/web` UI primitives. It is **not** part of the local Web UI product surface authors use. Authors do not receive a Design Studio nav item, route, or menu entry in Control Room, Setup, or desktop shell.

**Normative spec:** [`design-studio.md`](design-studio.md)

#### 30.1 DESIGN SSOT move (web consumer)

- Repo-root [`DESIGN.md`](../../../../DESIGN.md) + [`DESIGN.dark.md`](../../../../DESIGN.dark.md) are the sole normative DESIGN pair.
- `apps/web/DESIGN*.md` do not exist; `src/index.css`, `tailwind.config.ts`, and AGENTS references consume the root SSOT via `@nexus/design-tokens`.
- Token **names** are preserved verbatim to minimize CSS churn.
- `apps/web` behavior and author-visible UI remain the product contract in §§1–29; only the token **source path** differs.

#### 30.2 What design-studio is (and is not)

| Dimension | `apps/web` (this spec) | `apps/design-studio` |
| --- | --- | --- |
| Audience | Authors on local product | Contributors / frontend devs |
| Serving | Electron desktop bundle (packaged web dist); standalone dev/preview via Vite | Standalone `pnpm dev` only |
| Data | Daemon API / `NexusClient` | Static fixtures; no wire contracts |
| DESIGN role | Consumer | Read-only mirror + gallery |
| Packaged in the desktop bundle | Yes | **No** |

#### 30.3 Author invariants (unchanged)

- No new screens, routes, or settings in the author Web UI for design-studio.
- Setup wizard, Control Room IA (§29), and daemon status behavior unchanged by studio work alone.
- The first-launch/setup wizard reshape is §29.13; Design Studio remains a contributor gallery and does not deliver author onboarding.

#### 30.4 Contributor workflow (cross-reference)

Token tuning: edit repo-root [`DESIGN.md`](../../../../DESIGN.md) pair on disk → refresh design-studio → validate gallery → verify `apps/web` test/build. Full steps in [`design-studio.md`](design-studio.md) §5.2.

#### 30.5 Non-goals

- Bundling design-studio into desktop installer assets
- Exposing the studio URL in author-facing docs as a product feature
- Live token editor or YAML write-back from studio UI
- Unbounded migration of `components/ui/*` into `@42ch/nexus-ui`; only approved pure presentational primitives move through the component-promotion boundary.
- `wire_contracts_changed: false` — frontend/docs only
