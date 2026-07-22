# Spec: Desktop icons + residual slate

**plan_id:** `2026-07-22-v1.131-p3-desktop-icons-residuals`  
**Tracker:** `DF-V1131-DESKTOP-ICON`  
**Status:** specify+clarify+plan locked (architect Seat 2)

## Problem

Desktop Dock icon may still show pre-Chronos assets due to cache or stale `source-1024.png`. Open residuals from V1.130 and VI logo upgrade need slate-clear.

## Goals

1. Run `pnpm --filter desktop run icons:compose` and `pnpm --filter desktop run icons:generate`; commit updated LFS `source-1024.png` / preview **only if** the hash differs from the current committed asset. Verify the compose source is `logo-primary.svg` (Chronos primary plate).
2. Document the macOS Dock **cache-invalidation** step (restart Dock / rebuild app) in the desktop icons README, so a dogfooder who sees a stale pre-Chronos Dock icon knows how to force-refresh. This satisfies the manual portion of `R-VI-003`.
3. Close or re-defer residuals per the table below. **P3 does not implement code for residuals owned by P0/P1** — it only archives (if the owning plan closed them) or updates the deferral note (if the owning plan deferred).

### Residual slate (binding ownership)

| ID | Owner of the fix | P3 action | Intent |
|----|------------------|----------|--------|
| `R-VI-001` | frontend-dev (P1 T3 / P2 T3) | Close if dark Chronos gallery assertions added; else defer with owner | Add dark-only class assertions on primary button / token swatches |
| `R-VI-002` | frontend-dev (P1) | Close if P1 added setup/settings chronos-note fixtures; else defer | Add `*-chronos-note` fixtures if gallery intros expand |
| `R-VI-003` | ops-engineer (P2 T1) | Close: compose verified + Dock cache notes in README (manual smoke criteria documented) | Dock runtime visual smoke |
| `R-VI-004` | frontend-dev (P1 T2) | Close if P1 resized wordmark and human visual sign-off recorded; else defer | Wordmark visual polish sign-off |
| `R-VI-007…011` | frontend-dev / writing-specialist (P1 T4) | Close if P1 T4 edited the touched files; else defer with owner | JSDoc, comment drift, dead ternary, design-studio checklist count |
| `R-V1130P1-QC1-W-003` | **frontend-dev (P0 T3) — NOT P2** | Archive after P0 T3 closes the wire; missing closure blocks P3/iteration close. P3 does **not** wire. | Settings modal wire into Chronos titlebar |
| `R-V1130P1-QC1-S-001` | frontend-dev (P0 T2) | Archive after P0 Studio titlebar fixtures land; missing closure blocks P3/iteration close | Studio dual-pane/titlebar/modal fixtures |
| Other V1.130 low/nit | per-owner | Close cheap doc/test items touched by P0/P1/P2; defer hard ones with updated owner + trigger | Slate hygiene |

## Non-goals

- Full historical residual compaction across all plans
- Store marketing icons
- P2 wiring the Settings modal (owned by P0 T3)

## Acceptance

All AC are dogfood-testable or ledger-verifiable:

- **AC-1 (icon pipeline).** `pnpm --filter desktop run icons:compose` and `pnpm --filter desktop run icons:generate` exit 0. Desktop source PNGs match `logo-primary.svg` compose output (before/after hash record plus preview review). If hashes differ, the source and preview are committed together; if hashes match, no asset commit is needed and the match is noted.
- **AC-2 (Dock cache).** Desktop icons README contains a **cache-invalidation** section (restart Dock / rebuild app). A dogfooder following it can force-refresh a stale Dock icon. (`R-VI-003` manual criteria documented.)
- **AC-3 (residual ledger).** Every targeted ID in the table above is either `lifecycle: resolved` with an ND-A2 `closure_note` archived under `.mstar/archived/residuals/`, **or** remains open with an **updated deferral note** (owner + trigger). No targeted ID left indeterminate.
- **AC-4 (Settings boundary).** `R-V1130P1-QC1-W-003` and `R-V1130P1-QC1-S-001` are archived only with cited P0 implementation/QA evidence. Missing closure blocks P3 and iteration close; P2/P3 do not replace P0’s wire/fixture work.
- **AC-5 (tech debt rollup).** `metadata.tech_debt_summary` refreshed (counts only) via `tech-debt-rollup.sh` after archival/deferral.

## Architecture decision (locked)

### Source and generation chain

- Canonical vector input is `packages/nexus-ui/assets/logos/logo-primary.svg`; `apps/desktop/src-tauri/icons/source/compose-app-icon.mjs` is the sole vector→PNG composer.
- Canonical committed raster outputs are `source/source-1024.png` (Git LFS) and `source/app-icon-preview-256.png` (normal Git). Generated platform files remain build outputs and are not promoted to source-of-truth assets.
- Use the package-filtered commands because the repo root has no `icons:*` aliases. `icons:generate` currently composes again before `tauri icon`; the explicit compose command is retained as a review checkpoint, not a second implementation path.
- Record SHA-256 hashes for both committed rasters before compose and after generate. A changed `source-1024.png` and preview form one atomic asset change; an unchanged pair is documented as verification evidence.

### Bundle and runtime verification

- `tauri.conf.json` bundle icon paths continue to point at generated desktop outputs. The generation hook must run before bundle assembly.
- Add a README cache-invalidation sequence: quit all Nexus instances, rebuild/reinstall the `.app`, run `killall Dock` (macOS relaunches it), then relaunch Nexus. If LaunchServices still shows the old icon, remove the old app bundle before rebuilding/reinstalling.
- The preview checks composition only; the built `.app` Dock tile is authoritative for macOS mask/cache behavior. P3 is not complete on compose output alone.

### Residual boundary

- P3 never edits P0/P1-owned UI to manufacture closure. It archives a row only with cited implementation/QA evidence from the owning plan; otherwise it keeps the row open with an explicit owner and trigger.
- `DF-V1130-*` feature rows are must-ship and cannot be re-deferred by this plan. Only non-feature residuals in the binding slate may remain open under the owner+trigger rule.

## Validation

- Command evidence: filtered compose/generate exit 0, hashes recorded, generated bundle inputs present.
- Visual evidence: preview reviewed and rebuilt `.app` Dock tile passes after cache invalidation.
- Ledger evidence: each targeted residual has one deterministic outcome; archive notes cite closure evidence and the counts-only tech-debt rollup is refreshed after all moves.
