# P0 Dock icon squircle follow-up — RCA (V1.136 T1)

> **plan_id:** `2026-07-23-v1.136-p0-dock-icon-squircle-followup`  
> **worktree:** `.worktrees/v1.136-p0`  
> **branch:** `plan/v1.136-p0-dock-icon-squircle-followup`  
> **BASE_SHA:** `9ba517cd` (branch tip at evidence capture)  
> **captured:** 2026-07-23  
> **prior RCA:** [`.mstar/iterations/v1.135/guides/p1-dock-icon-rca.md`](../../v1.135/guides/p1-dock-icon-rca.md)  
> **spec:** [`.mstar/iterations/v1.136/specs/p0-dock-icon-squircle-followup.md`](../specs/p0-dock-icon-squircle-followup.md)

## Executive summary

| Field | Value |
|-------|-------|
| **Status** | **RCA evidence captured (H4/H5/H6) — Attempt 2 contrast bake landed; author Dock confirm PENDING** (`R-V1135P1-001`, `R-V1135P1-005` **open**) |
| **Primary cause (V1.136)** | **H6 — squircle bake was visually invisible** (Attempt 1: 0/4092 non-plate). **T2 fix:** contrasting `MARGIN_COLOR` `#1A4A66` vs plate `#0D2B3E` — Attempt 2 border scan **4092/4092** non-plate |
| **Contributing risk (H4)** | Only LaunchServices-indexed `Nexus.app` is the **main-checkout release bundle** (2026-07-22); worktree freshly generated `icon.icns` (2026-07-23) **differs** — author may eyeball a stale install |
| **H5** | **INCONCLUSIVE** — agent cannot complete quit → rebuild → `killall Dock` → relaunch on behalf of `@author` |
| **H7** | **Verified retained** — `pnpm dev:desktop` still runs `icons:generate` before `exec tauri dev` |
| **Next work (T2)** | ~~Contrast ≠ plate bake in `compose-app-icon.mjs`~~ **Done (Attempt 2)** — author ritual + P0G-1 eyeball still required |

V1.135 landed a squircle clip + 6% inset in compose (Task 2) and wired `icons:generate` into `dev:desktop` (H7). V1.136 **re-verifies** H4/H5/H6 per architect template. **H6 re-falsification:** the V1.135 geometry change does not produce any visible margin or corner contrast — the composed raster is indistinguishable from a full-bleed square plate. **Do not claim Dock Done** from this evidence alone.

---

## Hypothesis checklist (carried + V1.136 re-verify)

| ID | Hypothesis | V1.135 | V1.136 re-verify |
|----|------------|--------|------------------|
| **H1** | Transparent margin defeated squircle mask | **FALSIFIED** (baseline retained) | Carried — no regression |
| **H2** | Generated `.icns` / iconset defective | **FALSIFIED** | Carried — `icons:generate` exit 0; iconset members opaque |
| **H3** | Bundle ships stale `Nexus.icns` | **FALSIFIED** (at V1.135 capture) | **Re-opened risk** — main-checkout release `Nexus.icns` SHA ≠ worktree fresh `icon.icns` (see H4) |
| **H4** | Wrong/stale `.app` under test | **INCONCLUSIVE** | **ELEVATED** — single indexed bundle at main checkout; worktree has no release bundle |
| **H5** | LaunchServices cache beyond `killall Dock` | **INCONCLUSIVE** | **INCONCLUSIVE** — ritual not run by agent; extended `lsregister` steps documented below |
| **H6** | Plate geometry / margin contrast invisible | **CONFIRMED (primary)** | **RE-CONFIRMED** — margin = plate = `#0D2B3E`; squircle clip invisible on raster |
| **H7** | Dev path skipped `icons:generate` | **FIXED** | **VERIFIED** — wiring unchanged |

---

## Attempt 1 — 2026-07-23

| Field | Value |
|-------|-------|
| **Hypothesis** | H4 / H5 / H6 (mandatory re-verify) |
| **Build command** | `pnpm install --filter desktop...` then `pnpm --filter desktop run icons:compose` then `pnpm --filter desktop run icons:generate` |
| **Bundle path** | **Indexed (H4):** `/Users/bibi/workspace/organizations/42ch/nexus/apps/desktop/src-tauri/target/release/bundle/macos/Nexus.app` — **worktree release bundle:** _not built_ |
| **CFBundleIdentifier** | `io.nexus42.desktop` (from indexed bundle `Info.plist`) |
| **mdfind result** | `mdfind "kMDItemCFBundleIdentifier == 'io.nexus42.desktop'"` → **single result:** main-checkout path above. `mdfind "kMDItemDisplayName == 'Nexus.app'"` → same path. |
| **H4 duplicate check** | **Single bundle** in LaunchServices index. **Stale-install note:** indexed bundle `Nexus.icns` mtime **2026-07-22 20:41**; worktree `apps/desktop/src-tauri/icons/icon.icns` mtime **2026-07-23 18:41**; `icon_512x512@2x.png` SHA **differs** (`01812df1…` release vs `031aa563…` worktree generate). Author testing indexed bundle is **not** guaranteed to reflect current compose output. |
| **Cache ritual** | quit all → icons:generate → rebuild → killall Dock → relaunch — **no** (agent environment; no author eyeball) |
| **H5 extended** | `lsregister` reset — **no** (deferred to author if Attempt 1+T2 fail persists). Extended steps in § H5 below. |
| **H6 geometry** | Corner 5×5 samples: TL/TR/BL/BR **25/25 `#0D2B3E`** each. `marginMid` (inset/2, h/2) = `#0D2B3E`. `plateEdge` (inset,inset) = `#0D2B3E`. `innerCorner` = `#0D2B3E`. Center mark RGB `(17, 221, 233)`. Border scan: **0/4092** non-plate pixels. **margin≠plate: no**. Compose params unchanged from V1.135 (`PLATE_INSET_RATIO=0.06`, `SQUIRCLE_RADIUS_RATIO=0.22`, `PLATE_COLOR=#0D2B3E`, canvas background same). |
| **Author outcome** | **Pending** — `@author` squircle eyeball not recorded |
| **Next candidate** | **T2:** contrast ≠ plate bake (§ H6 T2 strategy). If author still sees square after T2 + ritual → **H8** asset-catalog / `@2x` scaling / macOS template rendering |

---

## Attempt 2 — 2026-07-23 (T2 contrast-margin bake)

| Field | Value |
|-------|-------|
| **Hypothesis** | H6 fix — contrasting opaque margin (Strategy A) |
| **Compose change** | `MARGIN_COLOR = '#1A4A66'` for canvas background + squircle clip flatten; `PLATE_COLOR = '#0D2B3E'` retained for inner plate raster |
| **Build command** | `pnpm --filter desktop run icons:compose` then `pnpm --filter desktop run icons:generate` |
| **Compose params** | `PLATE_INSET_RATIO=0.06`, `SQUIRCLE_RADIUS_RATIO=0.22`, `PLATE_COLOR=#0D2B3E`, `MARGIN_COLOR=#1A4A66` |
| **H6 geometry** | Corner 5×5 samples: TL/TR/BL/BR **25/25 `#1A4A66`** each. `marginMid` (inset/2, h/2) = `#1A4A66`. `plateInterior` (inset+200, inset+200) = `#0D2B3E`. `plateTopMid` (mid-x, inset+5) = `#0D2B3E`. Center mark RGB `(17, 221, 233)`. Border scan: **4092/4092** non-plate pixels. **margin≠plate: yes**. |
| **Author outcome** | **Pending** — `@author` squircle eyeball not recorded |
| **Next candidate** | Author ritual on **rebuilt** `.app` (§ Author Dock confirm). If fail persists → H5 extended `lsregister`, then **H8** |

---

## H4 evidence detail (bundle identity)

### Commands

```bash
mdfind "kMDItemCFBundleIdentifier == 'io.nexus42.desktop'"
# → /Users/bibi/workspace/organizations/42ch/nexus/apps/desktop/src-tauri/target/release/bundle/macos/Nexus.app

mdfind "kMDItemDisplayName == 'Nexus.app'"
# → same path

pgrep -fl nexus-desktop || echo "no nexus-desktop process"
# → no nexus-desktop process (at capture)

plutil -p …/Nexus.app/Contents/Info.plist | rg -i "CFBundle(Identifier|IconFile|Executable|Name)"
# → CFBundleExecutable => nexus-desktop
# → CFBundleIconFile => Nexus.icns
# → CFBundleIdentifier => io.nexus42.desktop
# → CFBundleName => Nexus
```

### Finding

LaunchServices indexes **one** `Nexus.app`, at the **main repo checkout** release path — not the V1.136 worktree. No `nexus-desktop` process was running at capture. **Risk:** author Dock eyeball on the indexed bundle tests a **2026-07-22** install while compose evidence was gathered from the **2026-07-23** worktree generate path. T2 must pair icon regenerate with a **documented rebuild** of the `.app` under test (see verify ritual).

---

## H5 evidence detail (Dock / LaunchServices cache)

### Standard ritual (normative — from spec)

1. Quit **all** `nexus-desktop` / Nexus instances.
2. `pnpm --filter desktop run icons:generate`
3. Rebuild/reinstall documented `.app` (`pnpm dev:desktop` **or** `pnpm --filter desktop run build`).
4. `killall Dock` (macOS relaunches Dock).
5. Relaunch Nexus; inspect Dock at normal size.

### Extended steps (if fail persists after T2)

1. Remove stale `Nexus.app` from prior install location (empty Trash if applicable).
2. Rebuild/reinstall from documented build command; repeat steps 2–5.
3. **LaunchServices reset** (destructive to LS database — author discretion):
   ```bash
   /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -kill -r -domain local -domain system -domain user
   killall Dock
   ```
4. Relaunch Nexus; re-check Dock tile.

### Finding

Agent **cannot** falsify H5 without `@author` completing the ritual on the **rebuilt** bundle. H5 remains **INCONCLUSIVE** for Attempt 1. If T2 contrast bake + rebuild still shows a sharp square, run extended `lsregister` before naming **H8+**.

---

## H6 evidence detail (geometry / contrast invisibility)

### V1.135 fix recap

`compose-app-icon.mjs` clips the plate to a 22% corner-radius squircle and centers it with 6% inset on a full canvas. V1.135 treated this as a visible squircle bake.

### V1.136 re-verification

```bash
cd apps/desktop
pnpm run icons:compose   # inset=61px radius=198px

node -e "
  // sharp sample script — see Attempt 1 table for results
"
```

| Sample point | RGB | Hex | Plate? |
|--------------|-----|-----|--------|
| Corner 5×5 (all four) | (13, 43, 62) × 25/25 | `#0D2B3E` | yes |
| `marginMid` (inset/2, mid-y) | (13, 43, 62) | `#0D2B3E` | yes |
| `plateEdge` (inset, inset) | (13, 43, 62) | `#0D2B3E` | yes |
| `innerCorner` (inset+10, inset+10) | (13, 43, 62) | `#0D2B3E` | yes |
| Center mark | (17, 221, 233) | cyan mark | no |
| Border scan (perimeter) | — | **0/4092** non-plate | — |

**Root issue (Attempt 1):** compose used `PLATE_COLOR` (`#0D2B3E`) for (a) canvas `background`, (b) `flatten({ background: PLATE_COLOR })` on the clipped plate, and (c) the SVG plate fill. The 6% “margin” and squircle-clipped corners were **the same color** as the plate — **margin≠plate: no**. The raster was visually identical to a full-bleed square plate.

**T2 fix (Attempt 2):** `MARGIN_COLOR` `#1A4A66` now fills canvas background and squircle-clipped corners; inner plate raster retains `#0D2B3E`. Automated samples: **margin≠plate: yes**; border scan **4092/4092** non-plate. Author Dock confirm still **PENDING**.

This matches spec anti-pattern §2: **same-color margin “fix”** — invisible to author.

### H6 T2 recommended compose change (do not claim Done until author confirms)

**Goal:** make outer tile geometry visible in the raster **before** macOS applies its mask.

**Strategy A — contrasting opaque margin (preferred for T2):**

```javascript
const PLATE_COLOR = '#0D2B3E';
const MARGIN_COLOR = '#1A4A66'; // MUST contrast with plate — tune in Studio VI-004 first

// Canvas create background: MARGIN_COLOR (not PLATE_COLOR)
// roundedPlate: clip squircle but flatten clipped pixels to MARGIN_COLOR at corners
//   OR composite rounded plate without re-flattening corners to plate on outer edge
// Verify with corner sample: marginMid MUST NOT equal plateEdge hex
```

**Strategy B — inner rim highlight:** keep plate fill; add 1–2px lighter inner stroke at squircle boundary (higher implementation cost; validate at 32px Dock size).

**Strategy C — template export:** export pre-masked squircle PNG from design tooling; compose only scales — defer unless A/B fail author gate.

**T2 acceptance for H6 fix (pre-author):** after compose, automated sample must show **margin≠plate: yes** (at least one perimeter/margin pixel differs from `#0D2B3E`). **P0G-1** still requires `@author` Dock eyeball after full ritual.

---

## H7 verification (retained)

| Path | `icons:generate` wired? |
|------|-------------------------|
| `package.json` `dev:desktop` | **yes** — before `exec tauri dev` |
| `apps/desktop/package.json` `predev` | **yes** |
| `tauri.conf.json` `beforeBuildCommand` | **yes** |

No regression detected in V1.136 worktree.

---

## Primary root cause (V1.136 selection)

**H6 — same-color squircle bake is invisible (re-confirmed).**

V1.135 correctly identified square-plate geometry as the class of problem and added squircle clipping, but the implementation fills margin and clipped regions with the **same** `#0D2B3E` as the plate. Pixel evidence shows no contrasting margin (0/4092 border non-plate). The author’s persistent sharp-square Dock tile is consistent with an icon that still **reads** as a full-bleed square plate.

**Contributing factor (H4):** the only indexed `Nexus.app` may be a **stale release build** (2026-07-22) relative to current compose output — author must rebuild and record the exact bundle path when confirming.

**H5** remains a secondary hypothesis until author completes the cache ritual on the rebuilt bundle.

**H8+** (asset catalog / `@2x` scaling / template rendering) — name only if T2 contrast bake + H4/H5 ritual still fail.

---

## Author Dock confirm block (P0G-1 / P0G-4)

> **Author Dock confirm: PENDING** — do **not** close `R-V1135P1-001`, `R-V1135P1-005`, or `R-V1134P1-001` until `@author` records Pass. Agents must not forge sign-off.

| Field | Value |
|-------|-------|
| **Gate status** | **PENDING** |
| Date | _@author — fill on confirm_ |
| Build command | _Record exact command — must rebuild `.app` after T2 compose; e.g._ `pnpm --filter desktop run icons:generate && pnpm --filter desktop run build` |
| Bundle path | _Record absolute path to `.app` under test — not assumed from mdfind_ |
| Cache ritual completed | _yes / no — quit → icons:generate → rebuild → `killall Dock` → relaunch_ |
| H5 extended (`lsregister`) | _yes / no if applied_ |
| Outcome | _Pass (squircle) / Fail (still square) — **not recorded**_ |
| Recorded by | _@author_ |

### Author verify checklist

| Step | Action |
|------|--------|
| 1 | Quit **all** Nexus / `nexus-desktop` instances. |
| 2 | `pnpm --filter desktop run icons:generate` (after T2 compose lands). |
| 3 | **Rebuild/reinstall** — record exact command and **absolute bundle path**. |
| 4 | `killall Dock` |
| 5 | Relaunch Nexus; confirm tooltip `nexus-desktop` / `io.nexus42.desktop`. |
| 6 | **Pass (P0G-1):** macOS squircle on outer tile. **Fail:** sharp square — residual stays open. |

---

## Anti-patterns confirmed

| Misread | V1.136 finding |
|---------|----------------|
| V1.135 squircle compose = visible rounding | Margin and plate same hex → **invisible** on raster and likely on Dock |
| mdfind single bundle = correct test artifact | Indexed bundle may be **stale** vs latest `icons:generate` |
| PNG / compose log = Dock Done | Author eyeball still **only** closure path (P0G-1) |
| Studio VI-004 = Done | CSS simulation ≠ live Dock mask |

---

## Residuals

| ID | Disposition |
|----|-------------|
| `R-V1135P1-001` | **Open** until P0G-1 `@author` Pass |
| `R-V1135P1-005` | **Open** — H6 subtlety (invisible same-color bake) documented; closes when contrast fix + author confirm |
| `R-V1134P1-001` | **Open** — closes with `R-V1135P1-001` |
