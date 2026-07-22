# Spec: Orchestration Settings rehome

**plan_id:** `2026-07-22-v1.130-p3-orch-settings-rehome`  
**Status:** specify+clarify+plan locked (architect Seat 2)  
**Wave:** 4 · blocked_by: P1 + P3a load green

## Problem

IA is mis-placed today:
- Compute lives under 编排 — but Compute is global config, not orchestration.
- Settings Profiles is global — but Profiles is profile/workspace choice that belongs under 编排.
- Settings is a full page — changing a setting is a context-losing detour, not a quick modal.

## Goals

- 编排 功能区 = menu items **minus Compute** (interim)
- Compute/Modules → Settings modal (global) — reuses P1 modal chrome (≥80% viewport desktop; ESC + click-outside for non-dirty; Seat 1 concern #4)
- Settings Profiles rename **工作区** → under 编排
- Primary Settings UX = **Settings modal** (≥80% viewport; deep links open modal sections)

## Non-Goals

- Orch load bugs (P3a); Orchestrator create+list 功能区 (menu is interim → V1.131)

## Architecture decision (locked)

- P1’s app-level Settings modal host exclusively owns chrome, ≥80vw × ≥80vh desktop sizing, responsive mobile layout, focus trap/restore, dismiss, dirty guard, and safe background route.
- Existing Settings section modules remain content-only pages. P3b adds section registry entries/extracts and must not introduce nested or parallel modal chrome.
- Modules/Compute reuses the existing query/detail behavior as a global Settings section. Remove Compute from 编排 only after the modal section is functional.
- Profiles is renamed 工作区 and mounted under 编排 功能区. Profile switching remains owned by the existing creator/profile coordinator; connection/path/advanced daemon settings remain global Settings content.
- Existing `/settings` deep links become compatibility adapters into the shared modal section registry. In-app opens preserve the prior non-settings location; direct loads use `/works`; unknown sections use the modal default. Close restores the safe background.
- All close vectors call P1 `requestClose`; dirty section registrations require discard confirmation and focus returns to the invoker.

## Dependency / wire

- Hard-blocked by merged P1 modal host and P3a green Strategy/Sessions/Modules load matrix.
- `wire_contracts_changed: false`

## Acceptance

Per compass AC **Settings rehome / 工作区 (P3b)** section. Plan-level DoD maps T1–T4 → AC Shell/IA settings + 工作区.

## Risks

Deep link breakage; modal a11y; route churn with P1 Settings modal host.
