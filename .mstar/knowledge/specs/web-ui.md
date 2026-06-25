# Local Web UI (Control Room + Setup) — Specification v1

**Status**: Shipped (V1.64) — Control Room + Setup MVP delivered (7 screen groups + TanStack Query data layer + F-P3/F-F1 adapters + W-1 error-envelope toasts + vitest baseline); daemon-served via rust-embed + SPA fallback (P3); QC tri-review Approve (Wave 2). Evidence: `apps/web/screenshots/`. Tauri desktop shell + content-authoring UI → V1.65+.  
**Document class**: Feature line  
**Created**: 2026-06-24  
**Scope**: Nexus local Web UI product contract — placement (`apps/web`), stack, daemon-served model, `tauri-api` adapter boundary, MVP surface (Control Room + Setup), Tauri / content-authoring roadmap, and strict separation from the private cloud SaaS  
**Iteration compass**: [v1.64-local-web-ui-kickoff-delivery-compass-v1.md](../../iterations/v1.64-local-web-ui-kickoff-delivery-compass-v1.md)

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
| **V1.64 (this spec)** | Control Room + Setup MVP (browser SPA), daemon-served via `rust-embed`, `tauri-api` adapter boundary frozen. |
| **V1.65** | (a) **Tauri desktop shell** (`apps/desktop`) — loads `apps/web/dist`, system webview, `TauriClient` impl, daemon hosting (sidecar `nexus42 daemon start` first; in-process lib link V1.66+); per-OS webview deps / signing / CI matrix. (b) **Content-authoring UI** first slice (chapter rich-text editor as the lead surface; outline/KB editors follow). (c) Findings-remediation UI + schedule/cron editor. |
| **V1.66+** | Mobile (Tauri v2 mobile targets); **F-P3** array-rename structural closure; **F-F1** server-side sort; `apps/web/DESIGN.md` → **Production** completeness level. |

The Tauri-ready boundary (§5) is what keeps V1.65 a thin shell rather than a rewrite.

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

*Local-first Web UI product contract. Draft in V1.64 Prepare; promotes to Shipped (V1.64) at P-last. Design tokens: `apps/web/DESIGN.md`; design intent input: [web-ui-design-requirements.md](web-ui-design-requirements.md).*
