# Spec — Orchestration IA + sessions + Dock icon (V1.120 P2)

**Status:** writing-specialist reviewed (Phase 1 §5.3) — pending PM §5.4 lock  
**Iteration:** V1.120  
**Plan:** `2026-07-17-v1.120-orchestration-ia-icon`  
**Feedback:** F3, F4, F7

## Problem

Orchestration → Sessions shows multiple “running” rows when the author has not started work (idle daemon). Orchestration → Capabilities is an internal mechanism page authors do not need. macOS Dock still shows nexus-desktop as a sharp white square instead of a squircle-masked logo.

## User value

Authors see only useful Orchestration surfaces: Sessions reflects live work honestly; internal capability registry is hidden; Desktop Dock looks native on macOS.

## Goals

1. Sessions list reflects **honest active runtime state** per product rule below.
2. **Capabilities soft-remove:** drop from Orchestration sidebar; `/capabilities` redirects to `/sessions`.
3. Desktop icon compose/generate yields transparent corners so macOS Dock applies system squircle (no opaque white plate).

## Product rule — “Running” in Sessions (locked, F3)

Sessions is an **active-work monitor**, not a session history browser.

| Term | Definition |
| --- | --- |
| **Active session** | Wire `status` ∈ `{ running, paused, waiting_for_input }` and session is returned by the active-session list API |
| **Shown on page** | Active sessions only — terminal states (`completed`, `failed`, `cancelled`) must not appear as live work |
| **Empty state** | When author has not started orchestration and daemon is idle → **zero rows**, empty state copy (“No active sessions”) |
| **Defect** | Any `running` badge when no orchestration is in progress |

**Status column:** Display the wire status string with existing `StatusBadge` humanization — product fix is **which rows appear**, not renaming statuses.

Implementation: daemon `_system.*` session filter (primary) + client defensive filter (secondary) per AD-P0-2 below; UI must meet AC-P2-1.

## IA decision — Capabilities soft-remove (locked, F4)

| Action | Required |
| --- | --- |
| Orchestration sidebar | Remove **Capabilities** nav item |
| Route `/capabilities` | Redirect to `/sessions` (replace or 302 — no 404) |
| Daemon APIs | **Keep** `GET /v1/daemon/capabilities` and registry — **no API deletion** |
| Product UI | Do not build capability-detail or admission UI |

## Non-goals

- Deleting or deprecating capability daemon APIs
- Redesigning Sessions into a live console or history timeline
- Windows/Linux icon redesign beyond shared transparent source asset
- Changing `web-ui.md` Orchestrator IA permanently (iteration-scoped nav change; durable normative update deferred to compound if needed)

## Acceptance criteria

| ID | Criterion |
|----|-----------|
| AC-P2-1 | With idle daemon and no author-started orchestration, Sessions shows **empty state** (zero rows) — documented repro from dogfood + test or filter assertion |
| AC-P2-2 | Orchestration sidebar has no Capabilities item |
| AC-P2-3 | Navigating to `/capabilities` redirects to `/sessions` (deep link does not 404) |
| AC-P2-4 | `icons:compose` output has transparent corners (alpha 0 outside mark); Dock smoke per `apps/desktop/src-tauri/icons/README.md` passes |
| AC-P2-5 | Sidebar nav and route tests updated for Capabilities removal + redirect |

## Copy requirements

Per DESIGN.md §Voice & Content: Title Case for empty-state titles; sentence case for descriptions.

| Surface | en intent | i18n |
| --- | --- | --- |
| **Sessions empty** | Title **No active sessions**; description explains sessions appear when runtime runs — honest when idle (zero rows) | Reuse `sessions.emptyTitle`, `sessions.emptyDescription` — no copy change required for F3 |
| **Capabilities** | No new user-facing “Capabilities” strings in Orchestration nav after P2 | Remove nav label only; do not add replacement copy |

## Architecture decisions (locked)

| ID | Decision |
| --- | --- |
| **AD-P0-2a** | Phantom “running” rows when idle are **`_system.*` daemon sessions** auto-started at boot (`boot.rs` WS-D: `scan_system_presets` → `start_session` per entry). `GET /v1/daemon/orchestration/sessions` already calls `engine.list_active` (non-terminal only) — not a client showing history. |
| **AD-P0-2b** | **Primary fix (daemon):** in `handlers/orchestration/sessions.rs` `list_sessions`, drop rows where `preset_id.starts_with("_system.")` before map/sort/paginate. Same response schema — `wire_contracts_changed: false`. |
| **AD-P0-2c** | **Secondary (web):** defensive filter in `useSessions` (`apps/web/src/api/queries.ts`) excluding `_system.*` preset ids — belt for older daemons + unit tests. |
| **AD-P0-2d** | **Deferred unless repro persists:** startup reconcile for SQLite-recovered non-terminal author sessions with no runner (orphans after crash). Document in P2 T1 if needed. |
| **AD-P2-1** | Capabilities soft-remove: remove `/capabilities` from `sidebar.tsx` orchestrator group + `root-layout.tsx` `MOBILE_NAV_KEYS`; add `<Route path="capabilities" element={<Navigate to="/sessions" replace />} />` in `App.tsx`. Keep `GET /v1/daemon/capabilities` and `CapabilitiesPage` source (unlinked). |
| **AD-P2-2** | Dock: run `icons:compose` + `icons:generate`; verify alpha-0 outside logo bounds per `apps/desktop/src-tauri/icons/README.md`. |

## Notes for implementers

- Update `sessions-page.test.tsx` with mock including `_system.maintenance` session → expect empty after filter.
- Update sidebar/route tests for Capabilities removal + redirect (AC-P2-5).
- Icon task is asset pipeline only — no Tauri config semantic change unless README requires it.
