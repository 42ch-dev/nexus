# Web Rebuild IA — App-Shell Architecture (Proposal)

| Attribute | Value |
| --- | --- |
| **Status** | **Proposed** — direction confirmed by the product owner against the clickable prototype (2026-10-22); not yet implemented |
| **Document class** | Product/UX architecture proposal |
| **Scope** | The information architecture and onboarding flow for the in-place rebuild of `apps/web`: app-shell layout, module-as-app rail, Creator-profile switcher, workspace switcher placement, first-run onboarding for both entrances |
| **Prototype** | `.mstar/projects/web-rebuild/prototypes/onboarding-v3.html` (clickable, verified in-browser; local-only, gitignored) |
| **Supersedes** | the retired web-ui spec (retired 2026-10-22; carried-over invariants are listed in §7) |
| **Coordinates with** | [canvas-strategy-surface.md](../../active/surfaces/canvas-strategy-surface.md) (canvas contracts, unchanged), [desktop-shell.md](../../active/surfaces/desktop-shell.md) (Electron host), [design-studio.md](../../active/surfaces/design-studio.md) (component gallery), [actor-product-model.md](../../active/architecture/actor-product-model.md) (Creator/Character model) |

## 1. Product framing

Nexus is a local-first, AI-driven narrative-orchestration studio. The one-line promise to creators: **your world remembers, so your story never contradicts itself**.

Two entrances share one shell:

| Entrance | Who | Relationship to the product |
| --- | --- | --- |
| **Content creator** (default, recommended path) | Novelists, tabletop GMs, worldbuilders — non-engineers | *Uses* the Creator agent: authors Worlds and Works, steers the AI |
| **Developer** | Preset/module authors, orchestration operators | *Cultivates* the Creator agent's capabilities: presets, WASM modules, capability registry, sessions |

Both identities operate the same underlying `creator_id`; the entrance only swaps the rail's app registry (§3).

## 2. Onboarding (first-run flow)

Design principles:

1. **Ask intent, not configuration.** Screen 1 is "what do you want to do", never "choose an entrance id".
2. **Defaults first, expert options collapsed.** Agent auto-scan with a recommended pick; workspace `creative_root` auto-derived; a creator can finish in ~20 seconds by pressing Next.
3. **The flow ends at the first story, not at "setup complete".** The last wizard step creates the first World and lands on its guided empty state (Timeline · Brief layer).
4. **Web is the full product surface.** The local service is invisible to the user; Electron is packaging only and runs the identical flow.

### 2.1 Creator path — 3 decision points

| Step | Screen | Content |
| --- | --- | --- |
| 1 | **Identity** | Value proposition + two cards: "我来写故事" (recommended) / "我来开发工具". Next disabled until picked. |
| 2 | **AI partner** | Auto-scan results (`AgentScanEntry`): recommended agent pre-selected; install guide and manual launch command behind collapsed links. No ACP/protocol vocabulary on this surface. |
| 3 | **First World** | Template cards (blank / fantasy / sci-fi; templates seed Brief + example timeline) + world name + default workspace note (`creative_root` derived; change-location behind "advanced"). **Create & enter** lands in the app shell. |

### 2.2 Developer path — 2 steps

Identity card "我来开发工具" → one merged step (agent picker + optional custom launch command + workspace root with Browse…) → lands in the developer shell (§4.2). The full technical panel is preserved, not simplified away.

## 3. App-shell IA (both entrances)

Three zones, one shared skeleton:

```text
┌──────────────────────────────────────────────────────┐
│ Topnav (48px)                                        │
│  N │ [Creator profile ▾]           🔍* 🌐 ☾ ⚙ [agent]│
├────────┬─────────────────────────────────────────────┤
│ App    │  Main surface (current app)                 │
│ rail   │                                             │
│ (dock) │                                             │
└────────┴─────────────────────────────────────────────┘
```

*🔍 = search, disabled placeholder until implemented.*

### 3.1 Topnav — identity, not navigation

- **Left**: small logo mark + **Creator profile switcher** (avatar + display name + dropdown: profiles with SOUL maturity, "New Creator…"). Backed by the existing surface: `GET /v1/daemon/creators`, `GET/POST /v1/daemon/creators/active`. Identical in both entrances.
- **Right**: search (disabled), language, dark/light theme, system settings entry, **agent chip** (live dot + agent name + version; click opens agent switching).

### 3.2 App rail — modules as apps

- A floating, translucent **dock** (blur, rounded, elevation), top-aligned; no bottom-pinned settings item (settings lives in the topnav).
- Each rail item is **one module = one app**: large icon + tiny label; active item highlighted.
- **Module = lazy boundary**: an app's route chunk and UI assets load only when the module is active. The rail is driven by an app registry per entrance.
- **Progressive activation (creator entrance)**: on first landing only 世界 (World) is active; 作品 / 会话 / 记忆 render locked (greyed, hover explains the unlock condition). Unlock events (e.g. first Brief saved → 作品 activates) fire a toast and light the item. Activation state is persisted per creator.

### 3.3 Workspace switcher — on the content surface, not the chrome

- `WorkspaceSummary{creator_id, workspace_slug, creative_root, display_name}` is a first-class daemon entity (`GET/POST /v1/daemon/workspaces` + active get/set) — **1 creator : N workspaces**, each with its own creative root.
- The switcher is a **chip row at the top of content apps** (世界/作品): one chip per workspace, click = `set_active_workspace`, trailing "＋新建工作区" chip = create (root auto-derived, advanced override). New creators see exactly one chip ("默认工作区") — zero overhead until needed.
- The developer entrance has no content surface and therefore **no workspace UI** (developers operate runtime objects, not creative content).
- The setup-time home directory remains the physical root; onboarding's first workspace writes its `creative_root` from it.

### 3.4 Concept map (locked vocabulary for the rebuild)

| UI element | Concept | Backing surface |
| --- | --- | --- |
| Topnav switcher | Creator profile (identity) | `/v1/daemon/creators[/active]` |
| Rail item | Feature module (app) | route-level lazy chunk + app registry |
| Chip row | Workspace (content container) | `/v1/daemon/workspaces` + set-active |
| World/Work lists | Content inside the active workspace | existing world/work endpoints, workspace-filtered |
| Setup home dir | Physical root (per creator) | home-layout; feeds first workspace `creative_root` |

## 4. Landing surfaces after onboarding

### 4.1 Creator lands in 世界 (World Timeline, Brief layer)

Guided empty state, not a blank dashboard:

- Layer tabs Brief / Narrative / Moment (Brief default).
- Empty-state card: "写下你的第一个 Brief" with example copy; inline editor on click; save renders the first Brief card.
- Side checklist as the progressive-onboarding spine: ① write the Brief → ② create the first Work bound to a World → ③ let the AI draft the first Narrative. Checklist progress and rail unlocks are the same events.

### 4.2 Developer lands in 操作台 (Hub)

Rail registry: 操作台 / Harness / 模块 / 能力 / 会话. Hub cards: local service status, agent (ACP) status, Harness summary (presets, running sessions), installed modules. Every rail app beyond Hub lazy-loads on first visit.

## 5. Rebuild execution notes

- **In-place, single SPA**: same `apps/web`, same build, no second scaffold (STRATEGY lock). Routes migrate screen by screen; the legacy tree is deleted as each replacement lands.
- **Component source**: all new screens compose from `@42ch/nexus-ui` (V1.148–V1.150 complete); Design Studio fixtures are the visual reference (`pnpm dev:design-studio`).
- **Web-first development**: the entire build/test loop runs in the browser against `apps/nexus-service`; Electron packaging is the last step and reuses the same SPA.

## 6. Non-goals

- No visual redesign, no new tokens/colors/typography — `DESIGN.md` / `DESIGN.dark.md` remain the sole token source.
- No second SPA, no new build target, no native player app (PD-09 unchanged).
- No new wire contracts for this IA — it consumes existing creators/workspaces/world/work surfaces. (If world/work list filtering needs a workspace query param, that is an additive daemon-api change owned by its own spec.)
- No cloud/SaaS feature, no auth surface (§7 carry-over).

## 7. Carried-over invariants from the retired spec

These survive the retirement of `web-ui.md` and bind the rebuild:

1. **PD-4 (Harness naming)** — user-visible pillar label is **Harness**; **Preset stays** the mechanism name; internal identifiers (`/strategies` route, `CanvasSurfaceKind = 'strategy'`, `preset` wire fields, CSS classes, hook names) are unchanged. recorded here as the durable authority.
2. **`NexusClient` boundary** — screen logic depends on the `NexusClient` interface, never on `fetch`/IPC/filesystem directly; browser and desktop transports are interchangeable implementations (retired spec §5).
3. **Cloud separation** — no cloud-product feature, platform auth, or platform-gated capability in this UI while `platform_integration = paused`; contract sharing is one-way (`schemas/` → `nexus-contracts`) (retired spec §2.2).
4. **AI owns prose** — no body full-text editor; the UI is the planning/structure surface, the runtime drafts body prose (retired spec §13.2).
5. **Local-first** — works fully offline; cloud sync opt-in; loopback service with no new auth surface.

## 8. Open decisions (to resolve during implementation)

| # | Question | Current lean |
| --- | --- | --- |
| O1 | `creative_root` derivation for onboarding's default workspace | `~/Nexus/<slugified-name>`; confirm against `home.rs` discovery |
| O2 | Does the legacy setup-time `workspaceRoot` become the first workspace's `creative_root`? | Yes; verify no consumer treats it as a global root |
| O3 | SOUL maturity % shown in the profile dropdown | Display-only; formula owned by creator-memory-soul-lifecycle |
| O4 | Locked-module unlock events beyond Brief → 作品 | Defined per app during its rebuild plan; persisted per creator |
| O5 | Group-chip row on list pages vs detail pages | List pages only; detail pages show workspace as breadcrumb context |
