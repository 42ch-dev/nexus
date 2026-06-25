# Local Web UI (Control Room + Setup → Content-Authoring) — Specification v1

**Status**: Shipped (V1.65) — Control Room + Setup MVP (V1.64) **+ Content-Authoring UI stage (V1.65, §13)**: outline rich-text editor + chapter structure table + structure CRUD (slug/wc/volume/status; title display-only) + body read-only render + browser "Copy path" context menu. Tauri desktop shell + body full-text editor + "open-with" → **V1.66** (compass §0 Q5). QC tri-review Approve (fix-wave-1) + QA Pass.
**Document class**: Feature line  
**Created**: 2026-06-24  
**Scope**: Nexus local Web UI product contract — placement (`apps/web`), stack, daemon-served model, `tauri-api` adapter boundary, MVP surface (Control Room + Setup), Content-Authoring stage (V1.65), Tauri / body-editor roadmap (V1.66), and strict separation from the private cloud SaaS  
**Iteration compass**: [v1.64-local-web-ui-kickoff-delivery-compass-v1.md](../../iterations/v1.64-local-web-ui-kickoff-delivery-compass-v1.md) (V1.64 ship) · [v1.65-outline-and-structure-authoring-delivery-compass-v1.md](../../iterations/v1.65-outline-and-structure-authoring-delivery-compass-v1.md) (V1.65 Content-Authoring stage — scope/roadmap SSOT)

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
| **V1.66** | (a) **Tauri desktop shell** (`apps/desktop`) — loads `apps/web/dist`, system webview, `TauriClient` impl, daemon hosting (sidecar `nexus42 daemon start` first; in-process lib link V1.67+); per-OS webview deps / signing / CI matrix. (b) **"Open with" / "Reveal in file manager"** desktop integration (Tauri `shell.open`/`openWith`). (c) **Body full-text editor + per-chapter edit lock** (coordinates with orchestration's host-tool write path). (d) Drag-to-reorder / bulk chapter ops / outline template library. |
| **V1.67+** | Mobile (Tauri v2 mobile targets); **F-P3** array-rename structural closure; **F-F1** server-side sort; remaining V1.64 Track-B carry-forwards; `apps/web/DESIGN.md` → **Production** completeness level. |

The Tauri-ready boundary (§5) is what keeps the V1.66 shell a thin wrap rather than a rewrite, and keeps the V1.66 body-editor a new screen rather than a re-architecture.

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

**Stage status**: **Draft** (flips to **Shipped (V1.66)** at P-last, on QC tri-review + QA pass).

### 14.2 The desktop loop this enables

1. **Launch** — double-click the `.app`; the window opens to the Control Room and the daemon starts transparently (no terminal, no port to remember).
2. **Work** — use the full V1.65 surface (Control Room + Setup + Outline/Structure Authoring) exactly as in the browser — same screens, same transport contracts.
3. **Reach the file** — right-click a chapter body or outline path → "Open with…" to pick a system markdown editor, or "Reveal in Finder" to jump to the file. Transparent daemon autostart is the larger *invisible* win; open-with/reveal is the one new *visible* capability.

### 14.3 Non-goals for V1.66 (durable V1.67+ roadmap)

- **Body full-text editor + per-chapter edit lock** — **V1.67 lead authoring slice**. Lock design + MD round-trip + frontmatter sync + conflict policy. V1.66 renders body read-only.
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
