# UI Continuity (V1.106 P2 Stretch)

**Status:** Draft — writing-complete (§5.3); PM lock pending §5.4  
**Tier:** Stretch (P2) — **may defer**; does **not** block iteration close  
**Plan:** `2026-07-10-v1.106-ui-continuity`  
**Compass:** `../v1.106-delivery-compass.md`

## Product outcome

When capacity allows, authors get clearer status badges, readable Select controls, simpler Settings navigation, and richer AgentPicker chrome — without blocking Must (P0+P1) delivery.

**User-visible win:** Status pills read by hue; Select chevrons don’t hug the border; Settings shows Agent and Workspace first; Agent cards feel polished.

## FB-V1106-001 — Badge Soft semantic contrast + Solid fill regression

**Problem:** Soft Badge variants can read as neutral gray; some Solid variants (`queued`, `warning`, `error`, `preset`) lost fill alignment with Soft hue family.

**User-visible outcome:** Authors distinguish running vs queued vs warning vs error at a glance in Control Room and Studio matrices.

### Acceptance

- [ ] All six Soft variants (`neutral`, `running`, `queued`, `warning`, `error`, `preset`) read as distinct hues in light and dark.
- [ ] Solid variants use fills from the same hue family as their Soft counterpart.
- [ ] Solid `queued`, `warning`, `error`, `preset` show visible fills (no “outline-only” regression).
- [ ] DESIGN.md `badge-status-pill` table updated if token values change.
- [ ] Studio Vitest solid variant matrix green (`packages/nexus-ui` + Studio consumer).

**SSOT:** `packages/nexus-ui/src/components/badge.tsx`

**Token owner (locked §5.2):** Root `DESIGN.md` + `DESIGN.dark.md` frontmatter `components.badge-status-pill` (`.soft` / `.solid` per variant). Any tint or fill change must update both theme files and pass WCAG 2.1 AA contrast floor (4.5:1 body text on soft fills; solid uses existing Button Contrast Invariant for text on fills).

## FB-V1106-002 — Select chevron right inset

**Problem:** Native `<select>` with symmetric `px-3` makes the chevron appear flush against the right border.

**User-visible outcome:** Closed, disabled, and invalid Select controls show clear space between chevron and border in Settings and Studio form fixtures.

### Acceptance

- [ ] Asymmetric horizontal padding and/or custom chevron overlay preserves native `<select>` contract (**no Radix Select**).
- [ ] `disabled` and `invalid` states inherit the same inset.
- [ ] App inherits package fix via `@42ch/nexus-ui` / `@/components/ui/select` wrapper without per-screen overrides.

**SSOT:** `packages/nexus-ui/src/components/select.tsx`

**Token owner (locked §5.2):** DESIGN.md `components.select` horizontal padding / chevron inset values. Implement via asymmetric `padding-inline` or overlay chevron in the package component — App wrapper inherits without per-screen overrides.

## FB-V1106-005 — Settings nav Advanced

**Problem:** Four top tabs (Agent, Connection, Setup, Workspace) bury Connection/setup concerns and overload primary nav.

**User-visible outcome:** Authors see **Agent**, **Workspace**, and **Advanced** only; Connection and Setup live as sections inside Advanced.

### Information architecture (locked)

| Top nav | Route | Contents |
|---------|-------|----------|
| Agent | `/settings/agent` | AgentPicker (unchanged body) |
| Workspace | `/settings/workspace` | Workspace path + browse (unchanged body) |
| Advanced | `/settings/advanced` | **Connection** section (`id="connection"`) + **Setup** section (`id="setup"`) on a **single page** — no nested `/settings/advanced/*` child routes |

### Route + redirect contract (locked §5.2)

- **Single route:** `/settings/advanced` renders both sections stacked (Connection first, Setup second) in `settings-advanced-section.tsx` (new) or equivalent module mounted at the Advanced outlet.
- **Hash anchors (recommended):** section `id` attributes enable deep links after redirect.
- **Redirects (Must when FB shipped):**
  - `/settings/connection` → `/settings/advanced#connection` (`replace`)
  - `/settings/setup` → `/settings/advanced#setup` (`replace`)
  - `/connect` → `/settings/advanced#connection` (`replace`) — supersedes prior `/settings/connection` landing for legacy entry
- **Client-context fingerprint mismatch:** update `client-context.tsx` bypass/redirect paths to target `/settings/advanced#connection` instead of `/settings/connection`.
- **Daemon health indicator link:** update `daemon-health-indicator.tsx` `Link` target to `/settings/advanced#connection`.

### Acceptance

- [ ] `settings-section-nav` shows exactly three tabs with Title Case labels: **Agent**, **Workspace**, **Advanced**.
- [ ] Advanced page renders Connection then Setup as distinct sections with headings.
- [ ] Studio `settings-host` fixtures demonstrate three-tab nav.
- [ ] App route tests cover redirects and active tab state.

### Voice & Content (when shipped)

| Surface | Element | Copy |
|---------|---------|------|
| Top nav | Tab labels | **Agent** · **Workspace** · **Advanced** |
| Advanced — Connection | Section heading | **Connection** |
| Advanced — Setup | Section heading | **Setup** |
| Legacy redirect | — | `/settings/connection` → `/settings/advanced#connection`; `/settings/setup` → `#setup`; `/connect` → `#connection` |

**Non-scope:** Multi-workspace switcher; wire changes.

## FB-V1106-006 — AgentPicker chrome polish

**Problem:** Section divider tight; Installed badge placement; outbound icon heavy; status dots subtle; uninstalled cards not muted enough.

**User-visible outcome:** Agent cards match V1.102+ polish intent in both Settings and wizard (shared component).

### Acceptance

- [ ] Section divider has breathing room above custom-launch block (spacing token, not magic px).
- [ ] **Installed** soft Badge sits beside agent title (not only in footer).
- [ ] Unified outbound **ArrowUpRight** icon tighter at label cap-height (CSS/`::after` preferred over duplicate SVGs).
- [ ] Selection status dots: selected = filled; unselected = hollow; visible in light/dark.
- [ ] Uninstalled agent **title** uses muted token; card remains discoverable (install/docs links).
- [ ] Studio-first: fixture accept before App claim.

### Voice & Content (when shipped)

| Element | Copy |
|---------|------|
| Installed badge (by title) | **Installed** |
| Uninstalled card title | Muted `gray-700` token — title still readable; links discoverable |
| Outbound docs link | Tighter **ArrowUpRight** at label cap-height |
| Status dot (selected) | Filled semantic dot |
| Status dot (unselected) | Hollow ring |

**SSOT:** `apps/web/src/components/setup/agent-picker.tsx` (app-shared; not package-promoted)

## Backlog note (not Must)

- **DF-70** execution-mode / BYOK matrix — deferred; do not silent-promote to Must.

## Non-goals (locked)

- Blocking iteration close if this plan is incomplete
- Wire / schema changes (`wire_contracts_changed: false`)
- Radix Select rewrite
- Package-promoting AgentPicker or SettingsShell
- DF-70 BYOK execution-mode matrix

## Architecture locks (§5.2)

See compass **Architecture Locks** — single `/settings/advanced` route, hash redirects, Badge/Select DESIGN token owners, `wire_contracts_changed: false`.

## Verification hooks (when picked up)

- `pnpm --filter nexus-ui test` — Badge/Select variant tests.
- `pnpm --filter web test` — settings shell nav + AgentPicker chrome.
- `pnpm --filter design-studio test` — settings-host + component matrices.
