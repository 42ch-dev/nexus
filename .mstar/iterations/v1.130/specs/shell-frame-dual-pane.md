# Spec: Shell frame — dual pane + titlebar + footer switch

**plan_id:** `2026-07-22-v1.130-p1-shell-frame`  
**Status:** specify+clarify+plan locked (architect Seat 2)  
**Wave:** 2 · blocked_by: P4 tokens (hard)

## Problem

The entry shell fights the author's mental model:
- Fixed left Menu + right Content; 创作|编排 as top sidebar tabs — author lands on a nav list, not a workspace.
- Settings is a full page — changing a setting is a context-losing detour.
- Content header wastes space.
- Profile is not auto-selected after init — author must click before doing anything.

## Goals

- Left **功能区** + right **内容区** layout chrome
- 创作|编排 switch on **功能区 footer** (retire top tabs)
- Desktop overlay titlebar: logo + Settings gear → opens **Settings modal** (≥80% viewport desktop; ESC + click-outside for non-dirty; content wiring may complete in P3b)
- Auto-select Default profile on cold start / after ensure-bootstrap (fallback bootstrap only if no Default) — **deterministic rule, Seat 1 concern #3**
- Studio fixtures light+dark before App claim

## Non-Goals

- Creator hub/entity content (P2); orch menu content moves (P3b); full Settings section migration (P3b)

## Architecture decision (locked)

### Shell and modal boundaries

- `RootLayout` owns dual-pane placement, overlay titlebar integration, and app-level overlay/provider mounts. Props-driven shell chrome lives under `components/layout/presentational/**` and is mirrored in Studio through `@web-layout/*`; route/data/Tauri behavior stays in App wrappers.
- One Settings modal host owns chrome, section outlet, focus trap/restore, scroll lock, URL section resolution, and dirty registrations. P3b contributes section content/registry entries only.
- Desktop modal dimensions are at least `80vw × 80vh`, bounded to the viewport; mobile is near-fullscreen. ESC, backdrop, close button, and route-close call the same `requestClose`.
- Clean content dismisses immediately. Dirty content registers with the host and requires explicit discard confirmation before any close vector succeeds.
- `/settings/:section` remains a compatibility deep link into the modal. In-app opens preserve the previous non-settings location; direct loads use `/works` as the background.

### Deterministic profile selection

1. Enumerate all creator-list pages and read the configured active creator through the existing active-creator endpoint (or use the ensure-bootstrap result during setup).
2. Select a profile whose trimmed display name equals `Default` case-insensitively; if multiple match, choose ascending `creator_id`.
3. If none matches, select the bootstrap/configured active creator when present in the list.
4. Only as defensive recovery, select the stable first `creator_id`.
5. Ignore a persisted localStorage id absent from the current list. Persist the resolved id; desktop invokes the existing active-creator switch before creator-scoped queries are enabled. Promoting the existing active-creator GET onto `NexusClient` is adapter work, not a schema change.

## Dependencies / wire

- P4 token projection is a hard prerequisite for P1 App paint. P0 is a soft integration dependency.
- `wire_contracts_changed: false`

## Acceptance

Per compass AC **Shell / IA frame (P1)** section. Plan-level DoD maps T1–T4 → AC Shell/IA frame portion.

## Risks

Tauri Overlay traffic-light inset; browser parity without overlay.
