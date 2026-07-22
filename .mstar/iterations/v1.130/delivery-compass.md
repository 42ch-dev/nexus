---
iteration_id: V1.130
start_date: 2026-07-22
end_date: 2026-07-22
status: completed
iteration_base_branch: main
spec_integration_branch: iteration/v1.130
target_branch: main
scale: XL
plans:
  - 2026-07-22-v1.130-p0-daemon-restart
  - 2026-07-22-v1.130-p4-vi-atmosphere
  - 2026-07-22-v1.130-p1-shell-frame
  - 2026-07-22-v1.130-p2-creator-dual-state
  - 2026-07-22-v1.130-p3-orch-load-repair
  - 2026-07-22-v1.130-p3-orch-settings-rehome
---

# V1.130 Delivery Compass

> **Phase 1 Review & Edit chain: COMPLETE — compass LOCKED (2026-07-22).**
> - Seat 1 (product-manager): User-value Scope + measurable AC groups; six handoff concerns for architect; Prepare gates → `ready for architect`.
> - Seat 2 (architect): All six Seat 1 concerns locked (Create World POST wire; 12-message Chat floor; Default→bootstrap profile order; Settings modal ≥80% + dirty guard; compile-time Chronos; endpoint-based load health). All six plans `plan: [locked]`.
> - Seat 3 (writing-specialist): Terminology + AC hygiene (功能区 footer; Settings modal; wire-only Create World). No `{KNOWLEDGE_DIR}` adds.
>
> **Prepare gates:** all six plans specify/clarify/plan = done/locked. Direction is locked — do not re-litigate grill locks or Seat 2 decisions.
> **Next:** Phase 2 Autonomous Execute on `iteration/v1.130` — Wave 1 = P0 ∥ P4.

## Scope

V1.130 rewrites the **Control Room entry shell** so an author opens the app and lands in a calm, focused 创作/编排 workspace — no transport friction, no office-navy visual drag, no full-page Settings detour. Four user pains drive the iteration:

1. **Restart distrust** — footer Restart today lies (`port already in use` when it isn't), then forces a delayed fullscreen recovery. Author loses flow.
2. **Office-navy visual drag** — `brand-deep-blue #1E3A5F` reads as traditional office software; author wants 生机 / 神秘 / 时间流 while keeping cyan `#25D1E0`.
3. **Entry-shell IA friction** — fixed left Menu + right Content; 创作|编排 as top sidebar tabs; Settings as full page; Profile not auto-selected. Author lands on a nav list, not a hub.
4. **编排 load lies + Compute misplaced** — Strategy / Sessions / Modules detail「无法加载」on healthy daemon; Compute lives under 编排 instead of global Settings; Profiles is global but should be 工作区 under 编排.

Spec points (user-value anchored):

1. Atomic / repeatable footer Restart (owned + attached; no false port conflict; mid-session in-place UX) — kills pain 1
2. Dual-pane shell (左功能区 + 右内容区); 创作|编排 on 功能区 footer; overlay titlebar logo + Settings; Settings modal (≥80% viewport); auto-select Default profile — kills pain 3
3. 创作 hub: **Create World** + **延续 World 开 Work**; entity: small Chat slot + action buttons (Chat depth thin/honest) — kills pain 3 (creator half)
4. Fix Strategy / Sessions / Modules detail load failures — kills pain 4 (load half)
5. Compute → Settings; Profiles → 工作区 under 编排 — kills pain 4 (IA half)
6. VI: re-hue `brand-deep-blue*`; lock **T1 Chronos** (Studio T2/T3 visible for pick); preserve `#25D1E0` — kills pain 2

## Plans

| Wave slot | plan_id | Name | Status | blocked_by | Wave |
|-----------|---------|------|--------|------------|------|
| W1a | `2026-07-22-v1.130-p0-daemon-restart` | Daemon Restart reliability | Done | — | 1 |
| W1b | `2026-07-22-v1.130-p4-vi-atmosphere` | VI Chronos lock | Done | — | 1∥ |
| W2 | `2026-07-22-v1.130-p1-shell-frame` | Shell frame | Done | P4 tokens (hard); P0 soft | 2 |
| W3a | `2026-07-22-v1.130-p2-creator-dual-state` | 创作 dual-state | Done | P1 | 3∥ |
| W3b | `2026-07-22-v1.130-p3-orch-load-repair` | 编排 load repair | Done | P1 (hard for implementation) | 3∥ |
| W4 | `2026-07-22-v1.130-p3-orch-settings-rehome` | Settings rehome | Done | P1 + P3a load green | 4 |

> **Slot vs nickname:** W3a/W3b are parallel Wave-3 *slots*. Plan nicknames P3a (load repair, slot W3b) and P3b (settings rehome, slot W4) come from the `p3-` plan_id prefix — do not conflate slot `3a` with plan `P3a`.

**PM order (HARD):** Wave 1 = P0 ∥ P4 → Wave 2 = P1 (after P4 token lock) → Wave 3 = P2 ∥ P3a → Wave 4 = P3b only when load AC green. Do not dispatch all six in parallel.

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit lock) | 2026-07-22 | done |
| Wave 1 (Restart + VI tokens) | TBD | pending |
| Wave 2 (Shell frame) | TBD | pending |
| Wave 3 (Creator + orch load) | TBD | pending |
| Wave 4 (Settings rehome) | TBD | pending |
| Iteration close | TBD | pending |

## Acceptance Criteria

**Restart (P0):**
- Footer Restart succeeds without false `port already in use` across 5 consecutive clicks on a freshly booted daemon, AND after CLI attach (second Restart is a real restart, not a no-op)
- Footer shows pending/fail state inline within 1s; mid-session failure does NOT require delayed fullscreen modal to recover
- zh-CN restart copy is honest (no "create profile" / "CLI-only" unless a true foreign-port conflict is detected)

**Shell / IA frame (P1):**
- No top sidebar 创作|编排 tabs; switch lives on 功能区 footer; dual-pane 功能区|内容区 visible
- Desktop: logo + Settings gear on overlay titlebar; Settings modal opens at ≥80% viewport; ESC + click-outside dismiss for non-dirty forms
- Cold start AND after ensure-bootstrap: Default profile auto-selected within 1s (fallback to bootstrap only if no Default exists); no manual click needed

**创作 hub / entity (P2):**
- Hub left: Create World + 延续 World 开 Work; right pane Worlds/Works lists only (no templates)
- Create World: user creates a World via typed `POST /v1/daemon/worlds`; success navigates to the new World timeline
- Entity open: small Chat slot (read-only, newest 12 messages, thin/honest) + action buttons (Agent, Outline/Timeline, Findings, back-to-hub); canvas/content on right

**编排 load (P3a):**
- Strategy canvas / Sessions list / Modules detail load on healthy daemon (daemon reachable + auth OK + engine running)
- When engine not running: honest empty / engine-unavailable state (NOT a load error)
- Must be green BEFORE P3b moves Modules into Settings modal

**Settings rehome / 工作区 (P3b):**
- Compute removed from 编排 menu; Compute/Modules wired into Settings modal (global)
- Settings Profiles renamed 工作区 and lives under 编排 功能区
- Full-page `/settings` demoted/redirected to modal primary; deep links still resolve

**VI (P4):**
- Product no longer reads as `#1E3A5F` office navy anywhere in shell chrome; cyan `#25D1E0` preserved
- T1 Chronos locked as compile-time default (no runtime switcher); Studio T2/T3 swatches visible for pick
- Tokens gallery updated with re-hued swatches; AA contrast documented for light + dark

## Non-Goals

- Templates gallery / Templates Tab
- Full Agent Chat product parity (deep ACP loop → V1.131)
- Orchestrator create+list 功能区 (menu interim)
- Timeline Moment / Canvas feature deepening
- Runtime multi-theme switcher (VI T1 Chronos is compile-time default; T2/T3 are Studio pick only)
- Platform / cloud settings; `DF-71` menu-bar daemon control
- OS-keychain pinning (`R-V192SEC-001` — deferred; tracked in P0 plan)

## Seat 1 — product-manager handoff (resolved)

Product decisions locked by grill/dogfood (do not re-litigate): Restart must be atomic+owned+attached; VI is Must with T1 Chronos default; hub = Create World + 延续 World 开 Work; right pane = Worlds/Works lists only (no templates); entity Chat = grill B (small Chat + action buttons, thin/honest).

Six handoff items below were open entering Seat 2; all are now locked in Seat 2:

1. **Create World wire vs honesty-degrade (P2 T2).** Product prefers the **wire path** (user can actually create a World this iteration). If `createWorld` / POST worlds domain rules block the wire, the **honesty-degrade** fallback is acceptable. Architect must lock the path early — it determines `wire_contracts_changed: true/false` and whether P2 needs schema work in Wave 3. **Product floor:** either path must let the author reach a "I have created a World" state without a console workaround.

2. **Chat "thin/honest" floor (P2 T4).** "Small Chat + action buttons" is locked, but the Chat floor needs a concrete definition so P2 does not drift into V1.131 Chat parity. **Product floor:** read-only recent message history (last N messages, N≈8–20) + a single input row that may be **disabled** when the ACP loop is not ready (honest disabled state, not a fake input). No streaming, no tool-call rendering, no multi-turn agent loop. Architect confirm this floor.

3. **Default profile auto-select order (P1 T4).** Compass says "Default (or bootstrap)". **Product rule:** prefer the **Default** profile (user-named default); fall back to bootstrap **only if no Default exists**. Architect confirm the deterministic selection order so P1 T4 has a single rule, not a heuristic.

4. **Settings modal "large modal" size floor (P1 T3 / P3b T1).** "Large modal" is locked but no size floor. **Product floor:** modal covers ≥80% viewport on desktop; ESC + click-outside dismiss retained for non-dirty forms; dirty-form guard before dismiss. Architect confirm so P1 modal shell and P3b Compute/Modules wiring agree on chrome.

5. **VI T1 Chronos default = compile-time, not runtime (P4 T2).** **Product intent:** ship with Chronos as the default; runtime switcher is a Non-Goal. Architect confirm the token lock is compile-time so P4 T2 does not accidentally build a switcher. T2/T3 remain Studio-pickable swatches only.

6. **编排 load "healthy daemon" contract (P3a).** AC says "on healthy daemon (or honest engine-unavailable)". **Product definition:** healthy = daemon reachable + auth OK + engine running. If engine not running, show honest empty (NOT a load error). Architect confirm the health-check contract so P3a RCA has a clear pass/fail and does not over-fix into engine-autostart territory.

## Seat 2 — architect decisions (LOCKED)

1. **Create World uses the wire path (P2 T2).** Add typed `POST /v1/daemon/worlds`; no honesty-only fallback. Request is `CreateWorldRequest { title }`. The daemon resolves the active creator, derives an ASCII kebab slug (`world` when the title yields no ASCII slug) with deterministic `-2`, `-3`, … collision suffixing, and applies `visibility=private` plus `time_policy=manual`. Success is `201 CreateWorldResponse { world_id, status }`; the client invalidates World lists and navigates to `/worlds/{world_id}/timeline`. The existing `nexus_local_db::narrative_write::create_world` remains the persistence/domain write boundary. This locks `wire_contracts_changed: true` for P2 and requires schema + generated Rust/TypeScript output in the same implementation commit.
2. **Chat floor is bounded and read-only (P2 T4).** Render the latest **12** messages in chronological order from already-available local history. The single input row is present but disabled with honest “Chat arrives in V1.131” copy. No send mutation, optimistic echo, streaming, tool-call rendering, composer state machine, or ACP loop belongs in V1.130. If no typed history source is already available, render an honest empty-history state rather than adding a Chat wire.
3. **Profile selection order is deterministic (P1 T4).** Enumerate all creator-list pages, and read the configured active creator through the existing active-creator endpoint (or the ensure-bootstrap result during setup). Choose the first profile whose trimmed display name equals `Default` case-insensitively; tie-break by ascending `creator_id`. If none exists, choose the bootstrap/configured active creator; only then use the first stable `creator_id` as a defensive recovery fallback. Persist the chosen id in `ActiveCreatorProvider`; desktop also invokes the existing active-creator switch boundary before dependent queries run. A stale localStorage id that is absent from the list is ignored.
4. **Settings modal chrome is shared and guarded (P1 T3 + P3b T1).** P1 owns one app-level modal host: desktop width/height are each at least `80vw`/`80vh` (bounded to the viewport), with focus trap, focus restore, scroll lock, and responsive near-fullscreen mobile behavior. ESC, backdrop, close button, and route-close share one `requestClose` gate. Non-dirty content closes immediately; dirty sections register with the host and require discard confirmation. P3b supplies section registry/content only; it must not fork modal chrome.
5. **Chronos is compile-time token data (P4 T2).** T1 values replace the existing stable token values in `DESIGN.md` / `DESIGN.dark.md` and their projections; no preference, persisted theme id, runtime branch, or switcher is added. T2 Umbra and T3 Aurora remain Studio-local comparison swatches and never enter runtime token exports. Cyan remains exactly `#25D1E0`; `#1E3A5F` is removed from product shell chrome and brand-token projections.
6. **Healthy-daemon/load classification is endpoint-based (P3a).** “Healthy” means the daemon health request succeeds, protected requests pass auth, and the endpoint-required engine/registry is present. Canonical `503 service_unavailable` for a missing engine maps to an honest engine-unavailable/empty state. Network/TLS/timeout, 401/403, 404, and non-engine 5xx remain distinct load errors. P3a fixes root causes and canonical error mapping only; it does not auto-start the engine or add a parallel aggregate health endpoint.

## Architecture Notes (LOCKED)

### Module boundaries

- **Restart:** `SidecarManager` owns one serialized `restart_daemon` operation across stop → port-free wait → spawn → health-ready. The web footer calls one desktop capability; it must not compose `stopDaemon()` + `startDaemon()`. Owned children stop through their handle. An attached listener is terminable only after Nexus health and PID/process identity verification; a foreign or unverifiable listener returns an honest conflict and is never killed. A restarted attached daemon becomes desktop-owned, making the next Restart identical to the owned path.
- **Shell:** `RootLayout` owns dual-pane placement and mounts providers/overlay hosts. Presentational shell chrome stays props-driven under `apps/web/src/components/layout/presentational/**` and is mirrored in Studio through `@web-layout/*`; routing, creator selection, Tauri lifecycle, and daemon state remain app-local.
- **Creator:** UI reads/writes only through `NexusClient`. The daemon handler owns request validation/defaults and delegates the write to `nexus_local_db::narrative_write`; React pages never derive ownership or write SQLite directly.
- **Settings:** the P1 modal host owns chrome, focus/dismiss behavior, dirty registration, and URL-to-section opening. Existing settings section modules remain content pages with no modal ownership. P3b moves Modules/Compute into the section registry and moves 工作区 into 编排; `/settings/:section` stays a compatibility deep-link adapter that opens the same modal section and preserves a safe background route.
- **Load repair:** query hooks and pages classify errors; daemon handlers own canonical status/error envelopes; orchestration engine/registry ownership stays in `WorkspaceState`. No client-side engine-autostart.
- **VI:** root DESIGN pair is normative → `packages/nexus-ui` brand constants/theme mirror → `tooling/design-tokens` CSS/Tailwind projection → Studio Tokens/Chronos fixtures → App consumption. Candidate shell fixtures are Studio-first; shell chrome remains an `@web-layout` presentational extract. No new `@42ch/nexus-ui` primitive promotion is planned.

### API / wire impact

- Iteration expectation: `wire_contracts_changed: true` **only because of P2 Create World**.
- Add `schemas/daemon-api/worlds/create-world-request.schema.json` and `create-world-response.schema.json`, codegen outputs, drift registration, daemon route/handler, `NexusClient.createWorld`, `BrowserClient.createWorld`, query mutation, and contract tests together.
- P0 uses Tauri IPC only (`wire_contracts_changed: false`). P1, P3a, P3b, and P4 remain false. P3a must stop and amend its plan before changing a daemon response shape; current expectation is repair against existing contracts.

### Wave / dependency rationale

- **Wave 1:** P0 ∥ P4 are independent (desktop lifecycle vs design-token chain).
- **Wave 2:** P1 may start only after P4’s Chronos token projection is merged. P0 is a soft integration dependency: it may finish alongside P1, but shell acceptance cannot close while Restart is red.
- **Wave 3:** P2 ∥ P3a start after P1 shell/modal boundaries are available. P3a’s prior “P1 soft” label is tightened to a hard implementation dependency to preserve the locked wave order; read-only RCA may be prepared earlier.
- **Wave 4:** P3b starts only after P1 modal host is merged and P3a’s Strategy/Sessions/Modules healthy + engine-unavailable matrix is green. This prevents moving a broken Modules surface and obscuring its root cause.

## Roadmap Position

- **Current (V1.130):** Shell entry rewrite + Restart + orch load/Settings rehome + VI Chronos Must — **delivered**
- **Next (V1.131):** Deepen Creator entity Agent Chat; Orchestrator 功能区 beyond menu; optional Umbra/Aurora VI tune; Create World slug collision suffixes; Settings modal full-page→modal adapter — trigger: V1.130 ship + author demand. Owner: product-manager
- **Final goal:** Author opens app → Default profile → Create World or continue Work → edit timeline without transport/restart/office-navy friction

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.130` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| XL scope slips | Med | High | Hard wave order; P3 split load→rehome; Chat thin (grill B) |
| Create World wire expands P2 | Med | Med | Keep the locked request title-only and reuse local-db write logic; any blocking domain invariant requires an explicit plan amendment, not silent honesty-degrade |
| VI + shell parallel thrash | Med | Med | P4 token lock before P1 App paint |
| Orch load RCA deeper than client | Med | Med | P3a RCA first; daemon fix only if proven |

## Iteration package

| Path | Purpose |
|------|---------|
| `specs/daemon-restart-reliability.md` | P0 |
| `specs/vi-atmosphere-chronos.md` | P4 |
| `specs/shell-frame-dual-pane.md` | P1 |
| `specs/creator-dual-state-entry.md` | P2 |
| `specs/orch-load-repair.md` | P3a |
| `specs/orch-settings-rehome.md` | P3b |
| `guides/` | Exploration notes |

## Grill decisions (locked)

| Q | Lock |
|---|------|
| Entity 功能区 Chat depth | **B** — small Chat + action buttons; thin/honest Chat |
| Hub create | **Create World** + **延续 World 开 Work** |
| Right pane templates | **A** — Worlds/Works lists only |
| VI | **Must** this iteration; default **T1 Chronos** |

## Seat 3 — writing-specialist note

Corpus pass complete. Terminology locks applied across compass, plans, and specs:

| Term | Canonical usage |
|------|-----------------|
| Shell layout | 左功能区 + 右内容区 (dual-pane) |
| Mode switch | 创作\|编排 on **功能区 footer** (not "Profiles footer" — survives P3b 工作区 rename) |
| Profile/workspace IA | **工作区** under 编排 功能区 (P3b); Default profile auto-select (P1) |
| Global config surface | **Settings modal** (≥80vw × 80vh desktop; shared `requestClose` + dirty guard) |
| VI default | **T1 Chronos** compile-time; T2 Umbra / T3 Aurora Studio-only |
| Create World | Wire path only (`POST /v1/daemon/worlds`); no honesty-degrade |
| Chat floor | Newest **12** messages, read-only; disabled input with V1.131 copy |

**Residual copy notes (non-blocking):**

- P3b renames Settings **Profiles** → **工作区**; pre-P3b docs may say "Profiles" only when referring to the current shipped label.
- "Orchestration" / "orch" remain acceptable in technical prose (plan_ids, crate names); author-facing copy uses **编排**.
- Seat 1 concern numbering is preserved in specs for traceability only.

No `.mstar/knowledge/` additions. No architect lock weakening.

## Phase 3 close placeholders

### Compound candidates

- Serialized daemon restart pattern (single-flight mutex + PID verification + port-free wait) — reusable for future lifecycle operations
- Design token re-hue methodology (find-replace across DESIGN.md → nexus-ui → design-tokens → Studio chain)
- Orchestration engine-unavailable classification pattern (503 canonical → UnavailableState; non-engine errors remain distinct)

### What shipped

- **P0:** Atomic/repeatable footer Restart (serialized `restart_daemon` with single-flight, PID verification, port-free wait, monitor race fix)
- **P4:** VI Chronos lock (brand-deep-blue re-hued from `#1E3A5F` office navy to `#0D2B3E` petrol-teal; cyan `#25D1E0` preserved; token chain fully updated)
- **P1:** Shell dual-pane IA (创作|编排 tabs moved to footer; Settings modal host ≥80vw; Default profile auto-select)
- **P2:** Create World wire path (`POST /v1/daemon/worlds` schema + codegen + daemon route + client + UI dialog)
- **P3a:** Orchestration load error classification verified (503 engine-unavailable → UnavailableState; regression test added)
- **P3b:** Settings rehome (Compute removed from 编排; Profiles renamed Workspace/工作区)

### Follow-ups

- Create World slug collision suffixes (deterministic `-2`, `-3`, …)
- Settings modal full-page → modal compatibility adapter (deep links)
- Default profile auto-select full spec compliance (pagination, desktop switchActiveCreator IPC)
- Studio fixtures for dual-pane/titlebar/modal (Studio-first policy)
- Entity Chat slot + action buttons (P2 T4)
- Continue World → Create Work binding (P2 T3)
- WCAG contrast table recomputation for Chronos values
- Daemon-side RCA for orchestration load failures (requires running daemon)
