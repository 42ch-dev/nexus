# Spec — Strategies repair (V1.120 P0)

**Status:** writing-specialist reviewed (Phase 1 §5.3) — pending PM §5.4 lock  
**Iteration:** V1.120  
**Plan:** `2026-07-17-v1.120-strategies-repair`  
**Feedback:** F5

## Problem

Authors opening any Strategy from Orchestration → Strategies hit `common.error.title` (“无法加载此视图”), with no way back to the list. System presets include `_system.*` internals authors never need. Validate is a global header action (wrong scope). User presets cannot be deleted from the UI although `DELETE /v1/daemon/presets/{id}` exists.

## User value

Authors can browse, open, validate, and delete their Strategies without dead-end errors or internal preset clutter.

## Goals

1. Opening a listed preset navigates to a working detail view (`getPreset` succeeds). On not-found or residual load failure, show in-page empty/error with **Back** — never a dead-end canvas `ErrorState` (`common.error.title`) as the only affordance.
2. System presets list excludes ids matching `_system.*` (prefix `_system.`).
3. Validate is a **per-row** action on each preset row (user / system / embedded as product allows).
4. User presets expose **Delete** with confirm dialog → `deletePreset` → list refresh.
5. Remove the global header **Validate** button and dialog entry point.

## Non-goals

- Redesigning the Strategy canvas graph editor
- New preset wire contracts or `@42ch/nexus-contracts` bump
- Deleting system or embedded presets
- Building preset edit/save forms (dirty-gate not in P0 scope)

## Product decisions (locked)

| Decision | Rule |
| --- | --- |
| `_system.*` filter | Apply to **System presets** section only; filter ids with prefix `_system.` |
| Validate placement | Per row (inline button or row overflow menu); **no** page-header Validate |
| Delete scope | User presets only; confirm before delete; no Delete on system/embedded |
| Detail failure UX | Primary fix: `locate_preset` for qualified `_system.*` ids (AD-P0-1b). Residual load failures use canvas `ErrorState` **plus** **Back** — not `common.error.title` alone |
| APIs | Reuse `NexusClient.validatePreset` / `deletePreset`; `wire_contracts_changed: false` |

## Copy & i18n requirements

Per DESIGN.md §Voice & Content: Title Case for buttons and dialog titles; sentence case for helpers and error bodies; verb-only CTAs.

| Surface | en intent | i18n |
| --- | --- | --- |
| **Back** | Verb-only `Back`; navigates to `/strategies` | Add `strategies.strategyDetail.back` (zh-CN equivalent) |
| **Not found** | Title Case title + sentence-case helper + **Back** | Reuse `strategies.strategyDetail.notFoundTitle` / `notFoundDescription`; add **Back** control |
| **Load error** | Canvas `ErrorState` may keep `common.error.title` (“Could not load this view”) for transport failures; **Back** must remain visible alongside retry if shown | `common.error.title`, `common.error.retry`; do not drop **Back** |
| **Delete confirm** | Dialog title names preset — e.g. **Delete Strategy** or **Delete "{{name}}"**; primary **Delete**, secondary **Cancel** | Add `strategies.deleteConfirm.title` (+ optional `{{name}}` interpolation); reuse `common.action.cancel` |
| **Validate (row)** | Verb-only **Validate** | Reuse `strategies.validatePreset` |
| **System presets empty** | Honest empty when `_system.*` filtered — existing copy acceptable | Reuse `strategies.systemPresets.empty` |

## Acceptance criteria

| ID | Criterion |
|----|-----------|
| AC-P0-1 | Clicking a row in User / System / Embedded lists opens detail with `getPreset` 200 for valid, non-filtered ids (no canvas `ErrorState` from `locate_preset` failure) |
| AC-P0-2 | Detail empty/error states render a **Back** control that navigates to `/strategies` without browser back |
| AC-P0-3 | System presets section never lists ids matching `/^_system\./` |
| AC-P0-4 | Each preset row exposes Validate (inline or overflow); page header has **no** global Validate button |
| AC-P0-5 | User preset rows offer Delete → confirm → `deletePreset` → list refreshes; system/embedded rows have no Delete |
| AC-P0-6 | Automated tests cover `_system.*` filter, Back navigation, per-row Validate placement, and Delete happy path (mocked client) |

## Architecture decisions (locked)

| ID | Decision |
| --- | --- |
| **AD-P0-1a** | The dogfood “无法加载此视图” title is **`common.error.title`** from `ErrorState` inside `StrategyCanvas` when `usePresetGraph` / `getPreset` fails — **not** a React Router route `errorElement` (none exists on `/strategies/:presetId`). |
| **AD-P0-1b** | **Primary root cause (code):** `locate_preset` in `preset_management.rs` resolves system presets with `presets/_system/{preset_id}/preset.yaml`, but list ids are **qualified** (`_system.maintenance`). Directory is `presets/_system/maintenance/`. Fix: strip `_system.` prefix (or delegate to `system_preset_dir::find_system_preset` / `resolve_preset`). |
| **AD-P0-1c** | **T1 repro matrix** before merge: (1) user preset open + `getPreset` 200, (2) embedded preset (e.g. `novel-writing`), (3) non-`_system.*` system preset if any remain listed. Record HTTP status + whether error is canvas `ErrorState` vs uncaught render. |
| **AD-P0-1d** | **Back control** lives on `strategy-page.tsx` for not-found **and** delegates to canvas load-error shell; navigates `useNavigate('/strategies')`. Do not rely on browser back. |
| **AD-P0-1e** | Keep route lazy split (`App.tsx` → `strategy-page` chunk). Do **not** move `@xyflow/react` into bootstrap. Add canvas-local error boundary **only** if T1 shows `buildStrategyGraph` / React Flow render throw after `getPreset` succeeds. |
| **AD-P0-1f** | List/detail boundary: list uses `listPresets` groups; detail uses `getPreset` + `parsePresetYaml` + adapter projection. List membership ≠ detail load success until `locate_preset` is fixed. |

## Notes for implementers

- Reuse `NexusClient.getPreset` / `validatePreset` / `deletePreset` — no schema change.
- `_system.*` list filter: client-side in `strategies-page.tsx` (`preset.id.startsWith('_system.')`) on **System presets** section only.
- Per-row Validate: inline or row overflow; remove header `ValidatePresetDialog` trigger.
- Tests: extend `strategy-page.test.tsx` for Back navigation; mock `getPreset` failure → Back visible; `strategies-page.test.tsx` for filter + row actions.
