# P1 Dock icon squircle — deep RCA (V1.135 T1)

> **plan_id:** `2026-07-23-v1.135-p1-dock-icon-squircle-rca`  
> **worktree:** `.worktrees/v1.135-p1-dock-icon-squircle`  
> **branch:** `plan/v1.135-p1-dock-icon-squircle-rca`  
> **BASE_SHA:** `2554cccd6922ff1560eea029a3b5e173962985e1`  
> **captured:** 2026-07-23  
> **prior RCA:** `.mstar/iterations/v1.134/guides/p1-app-icon-rca.md`

## Executive summary

| Field | Value |
|-------|-------|
| **Status** | **RCA complete — H6/H7 fixes landed (Task 2); author Dock confirm pending** (`R-V1134P1-001` open) |
| **Primary root cause** | **H6 — square plate geometry reads as sharp square under macOS mask** |
| **Contributing factor** | **H7 — `pnpm dev:desktop` bypassed `predev` / `icons:generate`** — **fixed (Task 2)** |
| **H1 baseline** | Retained and verified — opaque full-bleed holds; **not sufficient for closure** |
| **Fix applied (Task 2)** | Baked visible squircle rounding into compose (pre-rounded plate on opaque canvas) + wired `icons:generate` into `dev:desktop` path |

V1.134 correctly fixed transparent-margin mask defeat (H1) but the author still reports a sharp square Dock tile. This RCA falsifies H1–H3 as the ongoing primary cause: the compose → `tauri icon` → bundle pipeline ships a valid, opaque `.icns` identical between freshly generated artifacts and the release `.app`. The remaining best explanation is **H6**: `logo-primary-square.svg` rasterizes to a **full-bleed square `#0D2B3E` plate** edge-to-edge; even when macOS applies its squircle clip, the uniform dark square reads as a sharp-edged square plate (unlike peer apps whose artwork has internal padding or pre-rounded corners). **H7** is a secondary risk: the default `pnpm dev:desktop` entry path calls `exec tauri dev` and **does not** run the `predev` `icons:generate` hook.

---

## Hypothesis checklist (H1 → H7)

| ID | Hypothesis | Result | Evidence |
|----|------------|--------|----------|
| **H1** | Transparent / partial-alpha source defeated squircle mask | **FALSIFIED as primary** (baseline retained) | `source-1024.png`: 1024×1024, RGB, `hasAlpha: false`, `minAlpha=255`, `maxAlpha=255`. `app-icon-preview-256.png` identical. V1.134 compose retained. |
| **H2** | Generated `.icns` / iconset defective | **FALSIFIED** | After `pnpm --filter desktop run icons:generate`: 10 iconset members (16–512@2x). All members fully opaque (`minAlpha=255`, `zeroAlphaPixels=0`). `icon_512x512@2x.png` corner alphas all 255. |
| **H3** | Bundle ships stale / wrong `Resources/Nexus.icns` | **FALSIFIED** | Release `Nexus.app/Contents/Resources/Nexus.icns` member PNG hashes **byte-identical** to freshly generated `icon.icns` members. `CFBundleIconFile` = `Nexus.icns`. Container `.icns` SHA differs (metadata only); raster content matches. |
| **H4** | Dock shows wrong binary (duplicate / stale install) | **INCONCLUSIVE** | `mdfind kMDItemCFBundleIdentifier == 'io.nexus42.desktop'` → single bundle: `…/target/release/bundle/macos/Nexus.app`. No `nexus-desktop` process running at RCA time. Author test command not recorded. |
| **H5** | LaunchServices cache beyond `killall Dock` | **INCONCLUSIVE** | Cannot falsify without author running quit → rebuild → `killall Dock` → relaunch ritual on documented path. Extended cache steps documented for Task 4. |
| **H6** | Square plate geometry vs true mask failure | **CONFIRMED (primary)** | All four 5×5 corner samples = `#0D2B3E` plate (25/25 each). Border scan: 0/4092 non-plate pixels. SVG is 284×284 square rect fill. Dock peer apps use padded/rounded artwork; our full-bleed square plate dominates visual even if OS clip applies. |
| **H7** | `tauri dev` / debug bundle icon path mismatch | **FIXED (Task 2)** | Root `pnpm dev:desktop` now runs `icons:generate` before `exec tauri dev`. `pnpm dev:desktop:web` still triggers `predev` as before. Release bundle path verified. |

---

## Evidence detail

### Pipeline inspection

| Stage | Path / command | Finding |
|-------|----------------|---------|
| Compose | `apps/desktop/src-tauri/icons/source/compose-app-icon.mjs` | Pre-rounded squircle plate (6% opaque inset, 22% corner radius) on opaque `#0D2B3E` canvas — H6 fix (Task 2) |
| Generate | `pnpm --filter desktop run icons:generate` | Exit 0; produces `icon.icns`, `32x32.png`, `128x128.png`, `128x128@2x.png` |
| Bundle config | `tauri.conf.json` `bundle.icon[]` | `["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png"]` |
| Built plist | `Nexus.app/Contents/Info.plist` | `CFBundleIconFile` = `Nexus.icns`; `CFBundleExecutable` = `nexus-desktop`; `CFBundleIdentifier` = `io.nexus42.desktop` |
| Identity | Dock tooltip check (P1G-2) | `productName` = Nexus; executable `nexus-desktop` — matches author evidence |

### H1 commands

```bash
pnpm --filter desktop run icons:compose
sips -g all apps/desktop/src-tauri/icons/source/source-1024.png
# → pixelWidth: 1024, hasAlpha: no, samplesPerPixel: 3, space: RGB
```

### H2 commands

```bash
pnpm --filter desktop run icons:generate
iconutil --convert iconset --output /tmp/nexus-gen.iconset apps/desktop/src-tauri/icons/icon.icns
sips -g hasAlpha -g samplesPerPixel /tmp/nexus-gen.iconset/icon_512x512@2x.png
# → hasAlpha: yes, samplesPerPixel: 4 (RGBA container; all alpha values 255)
```

### H3 commands

```bash
iconutil --convert iconset --output /tmp/nexus-rel.iconset \
  apps/desktop/src-tauri/target/release/bundle/macos/Nexus.app/Contents/Resources/Nexus.icns
diff <(cd /tmp/nexus-gen.iconset && shasum -a 256 *.png | sort) \
     <(cd /tmp/nexus-rel.iconset && shasum -a 256 *.png | sort)
# → no diff (member PNGs identical)
plutil -p Nexus.app/Contents/Info.plist | rg -i icon
# → CFBundleIconFile => Nexus.icns
```

### H6 geometry (pre–Task 2 compose — falsification evidence)

Captured **before** Task 2 baked squircle rounding into compose:

```bash
# Corner plate-color check (node/sharp): all corners 25/25 #0D2B3E
# Border non-plate pixels: 0/4092
```

Corner RGB sample: `(13, 43, 62)` = `#0D2B3E` at all four corners; center mark RGB `(17, 221, 233)`. Post–Task 2 compose uses a pre-rounded squircle plate — **author Dock confirm still required** to validate live appearance.

### H7 dev-path gap (fixed Task 2)

```text
root dev:desktop → sidecar:dev + web build + icons:generate + exec tauri dev   # H7 fix
root dev:desktop:web → pnpm --filter desktop run dev      # runs predev → icons:generate
```

`beforeBuildCommand` in `tauri.conf.json` includes `icons:generate` for **release builds**; root `dev:desktop` now also runs `icons:generate` explicitly because `exec tauri dev` bypasses `predev`.

---

## Primary root cause (selected)

**H6 — full-bleed square plate geometry.**

The pipeline is technically correct post-V1.134 (H1 baseline, valid icns, bundle wiring). The composed asset is intentionally a **square plate filling 100% of the 1024×1024 canvas** (`logo-primary-square.svg` → opaque `#0D2B3E` rect). macOS squircle masking clips the outer boundary, but on a uniform dark square the clipped corners are indistinguishable from a “sharp square” plate — especially compared to peer Dock icons whose artwork includes internal margin or pre-rounded shape. Studio VI-004 simulates squircle via CSS `rounded-[22%]` on a preview frame; that simulation **does not prove** live Dock appearance and may have contributed to the V1.134 false-confidence closure.

**Contributing factor (H7 — fixed Task 2):** Authors using `pnpm dev:desktop` previously ran against icons that were never regenerated in that session because `predev` was bypassed. Root `package.json` now runs `icons:generate` before `exec tauri dev`.

---

## Fix applied (Task 2)

1. **Compose (H6):** `compose-app-icon.mjs` rasterizes the plate with **visible corner rounding baked in** — opaque squircle-shaped plate (22% corner radius) centered on `#0D2B3E` canvas with 6% opaque margin (not alpha).
2. **Dev path (H7):** Root `pnpm dev:desktop` runs `icons:generate` before `exec tauri dev`.
3. **Regenerate:** `pnpm --filter desktop run icons:generate` → rebuild `.app` → author ritual below.

---

## Author Dock confirm block (AC-4 / P1G-1 / P1G-5)

> **Author Dock confirm: PENDING** — do **not** mark plan Done, close **AC-4**, or close `R-V1134P1-001` until `@author` records Pass below. Agents must not forge sign-off.

| Field | Value |
|-------|-------|
| **Gate status** | **PENDING** (`R-V1134P1-001` **open**) |
| Date | _@author — fill on confirm_ |
| Build command | _Record exact command used — e.g._ `pnpm dev:desktop` _(dist-load; runs `icons:generate` first)_ _or_ `pnpm --filter desktop run icons:generate && pnpm --filter desktop run build` _then open_ `apps/desktop/src-tauri/target/release/bundle/macos/Nexus.app` |
| Cache ritual completed | _yes / no — quit → rebuild → `killall Dock` → relaunch_ |
| Outcome | _Pass (squircle) / Fail (still square) — **not recorded**_ |
| Recorded by | _@author_ |

### Author verify checklist (mandatory before judging)

Follow **in order**. Do not judge Dock shape from Studio VI-004, preview PNG, or opacity metadata alone.

| Step | Action |
|------|--------|
| 1 | Quit **all** Nexus / `nexus-desktop` instances (every process — not just close the window). |
| 2 | Regenerate icons: `pnpm --filter desktop run icons:generate` |
| 3 | **Rebuild/reinstall** the `.app` under test — record the exact command in the table above. Examples: `pnpm dev:desktop` (runs `icons:generate` first) **or** `pnpm --filter desktop run build` then open `apps/desktop/src-tauri/target/release/bundle/macos/Nexus.app`. |
| 4 | Restart Dock: `killall Dock` (macOS relaunches Dock automatically). |
| 5 | Relaunch Nexus; inspect Dock tile at normal size. Confirm tooltip/process is **Nexus desktop** (`nexus-desktop`, `io.nexus42.desktop`) — P1G-2. |
| 6 | **Pass (P1G-1):** macOS squircle rounding visible on outer tile boundary (like Safari/Settings). **Fail:** sharp 90° square outline — plan stays open; record outcome; escalate or name next candidate (H8+). |

### Outcome routing (fill when author confirms)

| Outcome | Next action |
|---------|-------------|
| **Pass** — squircle visible | Record date + build command above; close `R-V1134P1-001`; AC-4 / P1G-1 satisfied; plan may proceed to Done (subject to QC/QA). |
| **Fail** — still square | Leave `R-V1134P1-001` open; plan **not** Done; continue RCA or mark **Blocked** with exhausted H1–H7 + next candidate named. |
| **Pending** (current) | Checklist delivered (Task 4); awaiting `@author` eyeball — **no agent sign-off**. |

## Anti-patterns confirmed this iteration

| Misread | RCA finding |
|---------|-------------|
| Opaque PNG = Done | H1 passes but author square persists → opacity alone insufficient |
| Studio VI-004 = Done | CSS squircle simulation ≠ live Dock mask behavior |
| V1.134 DONE_WITH_CONCERNS = shipped | Author confirm never obtained; reopened in V1.135 |

---

## Residuals

| ID | Disposition |
|----|-------------|
| `R-V1134P1-001` | **Open** until author P1G-1 confirm above |
| `R-V1134P1-002` (Studio VI-004) | Supporting only |
