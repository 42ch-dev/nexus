---
iteration_id: V1.85
start_date: 2026-07-03
end_date: 2026-07-03
status: completed
iteration_base_branch: main
spec_integration_branch: iteration/v1.85
target_branch: main
plans:
  - 2026-07-03-v1.85-compass-and-plan-stubs
  - 2026-07-03-v1.85-app-icon-branding
  - 2026-07-03-v1.85-brand-and-ci-doc-hygiene
  - 2026-07-03-v1.85-closure
---

# V1.85 — App Icon Branding — Delivery Compass V1

**Status**: completed (2026-07-03). All 4 plans Done; QC consolidated Approve (3/3, clean first pass); QA Pass-with-deferred-GUI; 2 V1.84 residuals resolved; `wire_contracts_changed: false`. Phase 1 Review & Edit chain: `@product-manager` → `@architect` → `@writing-specialist` → PM lock. PR to `main` follows Phase 3.

## 0. Context

V1.83 established the Nexus logo family (`@42ch/nexus-ui` SVG variants + PNG source provenance under Git LFS) and the root `DESIGN.md` / `DESIGN.dark.md` brand SSOT. V1.84 consolidated the `apps/web` blue-scale token path to a single source and closed the CI/tooling hygiene residuals. Both shipped to `main` (V1.83 PR #110, V1.84 PR #111).

The next visible product-completeness gap is the **desktop app icon**. `apps/desktop/src-tauri/icons/` currently ships **placeholder icons** — the directory's own README states they are *"a placeholder generated from a solid-color source … Replace the source artwork before the first public release."* The macOS dock icon, Windows taskbar/installer icon, and bundle metadata all currently show placeholder artwork, not Nexus. The macOS dock icon and Windows taskbar/installer icon are the first OS-level brand surfaces a user sees when launching or installing the app; placeholder artwork breaks the visual identity established by the V1.83 logo family and V1.84 token consolidation. The `@tauri-apps/cli` (already a devDependency) provides `tauri icon <source-1024.png>` to regenerate every required OS icon format from one 1024×1024 source.

V1.85 closes that gap: compose a proper 1024×1024 app-icon source from the Nexus logo family (the mark on a branded background), regenerate all OS icon formats, update bundle references, and verify the desktop-bundle CI still produces a branded `Nexus.app`. As a companion hygiene track, it also closes the two low residuals V1.84's QC registered for V1.85+ (the `apps/web/DESIGN*.md` brand-blue prose drift, and the missing LFS-purpose comment in CI workflows). This completes the brand arc at the OS level: V1.83 assets → V1.84 tokens → V1.85 the icon users see in their dock.

No wire/schema/contract change, no daemon/local-API change, no `@42ch/nexus-ui` export-surface change.

## 1. Locked Decisions

| Decision | Resolution |
|---|---|
| Iteration direction | **A — App Icon Branding + V1.84 residual closure.** Replace placeholder Tauri app icons with Nexus-branded icons; close the two V1.84 low residuals. No React primitive extraction (deferred), no broader page redesign. |
| Branch policy | `iteration_base_branch=main` (HEAD post-V1.84); `spec_integration_branch=iteration/v1.85`; final `target_branch=main`. Documented project convention (`.mstar/AGENTS.md` + V1.39–V1.84 history). |
| Plan structure | P-1 Prepare → P0 App Icon Branding (headline) ‖ P1 Brand and CI Doc Hygiene (the two residuals) → P-last Closure. P0/P1 non-overlapping file sets → parallel. |
| Icon source approach | Deterministic vector composition from the V1.83 logo family (consistent with V1.83's "faithful vector redraw, not generative re-imagining" principle). Compose a 1024×1024 SVG (logo mark on the branded deep-blue/cyan background, with OS-icon-appropriate padding), rasterize to PNG, then `tauri icon` regenerates all icon formats. The SVG source is committed as normal git; the rasterized 1024×1024 PNG source is committed under Git LFS (per V1.83 binary provenance policy for logo-family PNGs). |
| Platform scope | All icon formats `tauri icon` emits: macOS `.icns` + PNGs, Windows `.ico` + StoreLogos, Linux, iOS/Android. macOS is the shipped/CI-built platform today; other formats are generated for completeness and future cross-OS releases (no new CI legs required this iteration). |
| Aesthetic review gate | The composed app-icon source (or a 256×256 downsized PNG render + optional dock tile mock) is presented by the P0 implementor (in the plan branch or PR comment) for explicit user aesthetic sign-off **before** running `tauri icon` and updating bundle refs. The artifact is the deterministic vector composition from the V1.83 logo family on the V1.84 brand palette. If the user explicitly defers or does not respond within the iteration window, the implementor proceeds with best-judgment composition, records "user deferred — best judgment applied" in `icons/README.md` and the commit message, and ships; the gate is satisfied by the presentation + deferral note (no blocking). |
| No new export surface | P0 does not change `packages/nexus-ui/package.json` `exports`. The new app-icon source assets (SVG + rasterized 1024×1024 PNG) live under `apps/desktop/src-tauri/icons/source/`; they are not a `@42ch/nexus-ui` public export. |
| No wire contracts | No `schemas/`, no `@42ch/nexus-contracts` bump, no daemon/local-API behavior change. `wire_contracts_changed: false`. |

## 2. Scope

This iteration locks two delivery specs plus prepare and closeout:

- **SP-1: App Icon Branding (P0 — user-visible headline).** Compose a 1024×1024 Nexus app-icon source (logo mark on branded background, deterministic vector composition from the V1.83 logo family; user aesthetic sign-off gate before regeneration — see §1), then run `pnpm --filter desktop exec tauri icon src-tauri/icons/source/source-1024.png` from `apps/desktop` to regenerate every OS icon format (macOS `.icns` + PNGs, Windows `.ico` + StoreLogos, Linux, iOS/Android). Update `apps/desktop/src-tauri/icons/README.md` to drop the "placeholder" wording, verify `tauri.conf.json` `bundle.icon` refs, and confirm a clean desktop-bundle build. Closes the placeholder-icon product gap. The dock/installer icons are the first OS-level brand surfaces users see.
- **SP-2: Brand and CI Doc Hygiene (P1 — companion).** Close `R-V184CL-QC1-S001` (update `apps/web/DESIGN.md` / `DESIGN.dark.md` brand-blue consumption-path prose to match the V1.84 single-sourced token reality) and `R-V184CL-QC2-S001` (add a one-line LFS-purpose comment at the CI `lfs: true` checkout steps). Doc/config only; no runtime change.
- **SP-3: Prepare (P-1).** Lock this compass and plan stubs; register in `status.json`; confirm Prepare gates.
- **SP-4: Closure (P-last).** QC tri-review + QA + compound + Profile B compaction + PR to `main`.

## 2.1 Architecture Hierarchy and Ownership

- **P0** owns `apps/desktop/src-tauri/icons/**` (regenerated icon formats + the new 1024×1024 app-icon source under `source/` + README) and `.gitattributes` (adds the app-icon-source LFS line). Source placement is `apps/desktop/src-tauri/icons/source/` (locked in §1 — desktop asset, not a cross-app brand export). SVG source normal git; rasterized 1024×1024 PNG source Git LFS (per V1.83 binary provenance policy). Regenerated small-format PNGs stay normal git. P0 must not edit `apps/web/**`, `.github/workflows/**`, or `packages/nexus-ui/package.json` `exports`.
- **P1** owns `apps/web/DESIGN.md`, `apps/web/DESIGN.dark.md` (brand-blue prose), and the `lfs: true` comment lines in `.github/workflows/ci.yml` + `.github/workflows/desktop-build.yml`. It must not touch `apps/desktop/**` or icons.
- **Cross-plan isolation:** P0 (`apps/desktop/src-tauri/icons/**` + `.gitattributes`) and P1 (`apps/web/DESIGN*` + `.github/workflows/*.yml`) have non-overlapping file sets → safe parallel branches. `.gitattributes` is owned by P0 (adds the app-icon-source LFS line); P1 does not touch it. The only shared file is the integration branch at merge.
- **DESIGN.md discipline:** the root `DESIGN.md` / `DESIGN.dark.md` remain the brand SSOT. P1 only amends `apps/web/DESIGN*.md` *consumption* prose (the drift), not brand values.

## 2.2 Product Success Criteria

- The macOS dock / `Nexus.app` icon (and other OS icon formats) is recognizably Nexus: the committed 1024×1024 app-icon source uses the V1.83 logo mark on the V1.84 brand deep-blue/cyan palette; `tauri icon` has regenerated all shipped icon formats; bundle refs resolve; a desktop-bundle build succeeds. (Evidence: committed source SVG + `source-1024.png`, emitted icon files, successful `pnpm --filter desktop exec tauri icon src-tauri/icons/source/source-1024.png`, and bundle build.)
- The aesthetic review gate has been executed: the composed app-icon source (or downsized render) was presented to the user for sign-off before regeneration, or explicit deferral was recorded in `icons/README.md` + commit message.
- `tauri icon` regeneration is reproducible from the committed source (a future logo tweak → one command → all icon formats refreshed).
- The two V1.84 low residuals are closed (`lifecycle: resolved`).
- No wire/schema/contract change and no `@42ch/nexus-ui` export-surface change.
- Desktop-bundle CI still produces a valid (now branded) `Nexus.app`.

## 3. Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-03-v1.85-compass-and-plan-stubs` | P-1 — Prepare (Compass and Plan Stubs) | Done | Compass locked + 4 plans registered + `status.json`; Phase 1 Review & Edit chain. QC skipped (prepare/harness-docs). |
| `2026-07-03-v1.85-app-icon-branding` | P0 — App Icon Branding | Done | Impl `dc0c8057` (merged `d4bb8a92`); composed 1024 source + `tauri icon` regenerated all OS formats; README rewritten; LFS line added; 256px preview. QC Approve. |
| `2026-07-03-v1.85-brand-and-ci-doc-hygiene` | P1 — Brand and CI Doc Hygiene | Done | Impl `d0982aa9` (merged `030d71c6`); closed R-V184CL-QC1-S001 + R-V184CL-QC2-S001. QC Approve. |
| `2026-07-03-v1.85-closure` | P-last — QC, QA, Compound, and Closeout | Done | Consolidated QC Approve (3/3 clean); QA Pass-with-deferred-GUI; compound none (see §10); Profile B pending PR merge. |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## 4. Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Phase 1 scope and plan lock | 2026-07-03 | complete |
| P0 app icon source + regeneration | 2026-07-03 | complete |
| P1 doc hygiene complete | 2026-07-03 | complete |
| QC/QA and iteration close | 2026-07-03 | complete |

## 5. Acceptance Criteria

- The committed 1024×1024 app-icon source (e.g. `apps/desktop/src-tauri/icons/source/source-1024.png`) is Nexus-branded: deterministic vector composition of the V1.83 logo mark on the V1.84 brand palette (deep-blue `#1E3A5F` background + cyan `#25D1E0`/white N); verified by inspecting the source SVG/PNG.
- Aesthetic review gate executed: the app-icon source (or 256×256 downsized render) was presented for user sign-off before `tauri icon` (or explicit deferral recorded in `icons/README.md` + commit message).
- `tauri icon` regenerated every icon format the repo currently ships (`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico`, `icon.png`, Windows `Square*`/`StoreLogo`, iOS/Android) — verified by the command emitting them and the bundle refs resolving.
- `apps/desktop/src-tauri/icons/README.md` no longer says "placeholder"; documents the source + regeneration command.
- `tauri.conf.json` `bundle.icon` refs still resolve to valid branded PNGs; a desktop-bundle build (local + CI) succeeds.
- `.gitattributes` has `apps/desktop/src-tauri/icons/source/*.png filter=lfs diff=lfs merge=lfs -text` (binary source PNG → LFS; regenerated small-format PNGs stay normal git, consistent with V1.83 policy).
- `R-V184CL-QC1-S001` resolved: `apps/web/DESIGN.md` / `DESIGN.dark.md` brand-blue prose matches the V1.84 single-sourced token reality.
- `R-V184CL-QC2-S001` resolved: `lfs: true` checkout steps carry a one-line purpose comment.
- `wire_contracts_changed: false`; no `@42ch/nexus-ui` `package.json` `exports` change.

## 6. Non-Goals

- React primitive extraction into `@42ch/nexus-ui` (deferred to a later iteration).
- Any root `DESIGN.md` / `DESIGN.dark.md` brand-value change (brand SSOT untouched; P1 amends only `apps/web` consumption prose).
- Re-imagining the logo concept beyond faithful composition of the V1.83 mark.
- New CI legs for non-macOS desktop builds (other-OS icon formats are generated for completeness; no new build matrix).
- Any `schemas/`, wire-contract, or `@42ch/nexus-contracts` change.
- Any daemon / local-API behavior change.
- Mobile (iOS/Android) app packaging — only the icon assets `tauri icon` emits; no mobile build pipeline.

## 7. Roadmap Position

- **Current iteration (V1.85) — delivered**: Completed the brand arc at the OS level — the desktop app icon (macOS dock, Windows taskbar/installer, Linux, iOS/Android formats) is now recognizably Nexus (1024×1024 source composed from the V1.83 logo mark on the V1.84 brand palette, regenerated via `tauri icon`). Closed the 2 V1.84 low residuals (DESIGN.md brand-blue prose drift + LFS-purpose CI comments). QC consolidated Approve (3/3 clean); QA Pass. V1.83 (assets) + V1.84 (tokens) + V1.85 (OS icon) together establish the durable, cross-surface brand foundation.
- **Next iteration**: React primitive extraction into `@42ch/nexus-ui` (Button/Card/Badge/…) now that the brand foundation is fully laid, OR the next product feature loop. Trigger: V1.85 PR merged + branded bundle verified. Owner: `@project-manager`.
- **Final goal**: One cross-application Nexus brand system — assets, tokens, and OS-level identity — with app-specific implementations consuming shared foundations rather than inventing local visual language.

## 8. Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `.mstar/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.85` |
| `target_branch` | `main` |

Plan topic branches merge back to `iteration/v1.85`; the final PR targets `main`. QC/QA review the integrated `iteration/v1.85` HEAD. Per `.mstar/AGENTS.md`: `metadata.integration_branch=iteration/v1.85`, plan `working_branch=feature/v1.85-<plan-slug>`, plan `merge_target=iteration/v1.85`.

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| App-icon aesthetics miss the mark (padding/background/dock presence) | Medium | Medium | Aesthetic sign-off gate (§1): P0 implementor presents the app-icon source (or 256×256 render + dock mock) for explicit user sign-off before `tauri icon`; deferral note satisfies gate (no block); deterministic vector composition from V1.83 mark keeps it on-brand. |
| `tauri icon` fails or emits unexpected formats | Low | Low | `@tauri-apps/cli` is already a devDep and the documented command; verify locally before committing; keep the README's regenerate instructions accurate. |
| macOS squircle mask crops the mark awkwardly | Medium | Medium | Compose with OS-icon padding (Tauri applies the platform masks); render a downsized preview for sign-off. |
| Branded PNGs bloat the repo / LFS scope | Low | Low | Commit the binary 1024×1024 app-icon source under Git LFS (consistent with V1.83 PNG policy); regenerated format PNGs stay normal git (small). Verify `.gitattributes` scope. |
| Desktop-bundle CI breaks on new icons | Low | Low | Icons are drop-in replacements of the same filenames; bundle refs unchanged; CI desktop-build verifies. |
| P0/P1 conflict at integration merge | Low | Low | Non-overlapping file sets: P0 owns `icons/**` + `.gitattributes`; P1 owns `apps/web/DESIGN*` + `workflows/*.yml`; resolve any drift on `iteration/v1.85`. |
| Doc-drift fix (P1) over-reaches into brand-value edits | Low | Low | P1 amends only `apps/web/DESIGN*.md` consumption prose; root DESIGN SSOT untouched; QC1 checks. |

## 10. Compound Round Summary

- Crystallized docs: 0
- New CONCEPTS.md entries: 0
- Triggered compound-refresh: no
- **No crystallizable knowledge this round.** Two candidates assessed against `mstar-compound` Q1–Q8:
  1. **`tauri icon` regeneration workflow (P0):** the source-under-LFS + `pnpm --filter desktop exec tauri icon <src>` workflow is now documented in `apps/desktop/src-tauri/icons/README.md` itself (the natural home for a maintainer runbook). Crystallizing it into `{KNOWLEDGE_DIR}` would duplicate the README; not a new reusable cross-cutting rule (Q5 overlap with the README; Q6 = no architecture decision).
  2. **sharp-cli SVG-overwrite pitfall (P0 dev note):** `sharp-cli` silently overwrote `app-icon.svg` with PNG bytes on a first attempt — a "what didn't work". It is a general tool behavior (Q4 = No, not project-specific) and single-file (Q8 = No); searchable externally.
  Both scored ≤ Q4/Q6 — skip per the decision matrix. Recorded here per `mstar-iteration` §3.2.

## 11. Iteration Retrospective (Minimal)

- What worked:
  - Architect verification during Phase 1 caught the wrong `tauri icon` command path *before* implementation (`--filter desktop` sets cwd to `apps/desktop`, so the source path must be `src-tauri/...`, not `apps/desktop/src-tauri/...`) — the frontend-dev implemented the correct command first try.
  - P0/P1 parallel via isolated git worktrees on non-overlapping file sets merged cleanly (zero conflicts).
  - QC tri-review passed clean on first pass (3/3 Approve, no fix-wave) — the small, well-scoped, asset+docs-only change class was low-regression-risk; qc2 confirmed the LFS pointer was valid (no binary leak into git history) and the SVG carried no `<script>`/external refs.
  - The aesthetic-deferral gate (best-judgment composition + 256px preview for PR review) kept the iteration autonomous while preserving user review at PR time.
- What could improve:
  - The first parallel P0/P1 dispatch (V1.84) had aborted once; V1.85's parallel dispatch succeeded first try — but the pattern still carries some host-dispatch fragility for tiny parallel tracks.
  - The 256px preview can't fully convey the macOS squircle/dock rendering; a future brand-asset task could generate a mock dock-tile render for sign-off.
  - The LFS-purpose CI comment (R-V185CL-QC1-S001) now covers two asset roots (logo PNGs + app-icon source) but the comment text enumerates only one — trivial doc accuracy, accepted as low residual.
- Next iteration suggestion: React primitive extraction into `@42ch/nexus-ui` (Button/Card/Badge/…) now that the brand foundation (assets + tokens + OS icon) is fully laid, OR the next product feature loop — per §7 roadmap trigger.
