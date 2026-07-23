# P1 app icon full-bleed — RCA (T1)

> Promoted to: see Compound Round Summary in delivery-compass.md

**plan_id:** `2026-07-23-v1.134-p1-app-icon-full-bleed`  
**worktree:** `plan/v1.134-p1-app-icon-full-bleed`  
**captured:** 2026-07-23  
**task:** Rebuild opaque full-bleed app icon; confirm macOS squircle rounding

## Executive summary

| Field | Value |
|-------|-------|
| **Status** | **DONE_WITH_CONCERNS** — technical rebuild + opacity proof complete; **author Dock visual confirm pending** |
| **Hypothesis** | Transparent alpha border (~12% inset per side) in the prior compose pipeline defeated macOS squircle masking, yielding a flat square Dock tile |
| **Fix applied** | `compose-app-icon.mjs` now rasterizes `logo-primary-square.svg` full-bleed at 1024×1024 with `.flatten({ background: '#0D2B3E' })` — no transparent margins |
| **Opacity evidence** | Both `source-1024.png` and `app-icon-preview-256.png` are fully opaque (3 channels, `hasAlpha: false`, min/max alpha 255) |
| **Author Dock confirm** | **Pending** — requires author to quit Nexus, rebuild/reinstall, `killall Dock`, relaunch, and inspect Dock squircle |

## Step 1 — SVG opacity check

`packages/nexus-ui/assets/logos/logo-primary-square.svg` includes an explicit full-canvas background:

```xml
<rect width="284" height="284" fill="#0D2B3E"/>
```

The plate is opaque Chronos deep blue; rasterizing at 1024×1024 with `fit: 'fill'` + flatten yields a borderless opaque PNG.

## Step 2 — Compose script change

**Before:** `INSET_RATIO = 12/96` (~12% transparent margin per side); plate rasterized smaller and composited onto a transparent 1024×1024 canvas.

**After:** Direct full-bleed rasterize + flatten onto `#0D2B3E`. Comment documents the opaque-full-bleed rule and why (macOS masks opaque full-canvas icons; transparency defeats the mask).

File: `apps/desktop/src-tauri/icons/source/compose-app-icon.mjs`

## Step 3 — Regeneration + opacity proof

```bash
pnpm --filter desktop run icons:compose
pnpm --filter desktop run icons:generate
```

**sharp metadata (2026-07-23):**

| File | Size | Channels | hasAlpha | minAlpha | maxAlpha | fullyOpaque |
|------|------|----------|----------|----------|----------|-------------|
| `source/source-1024.png` | 1024×1024 | 3 | false | 255 | 255 | **yes** |
| `source/app-icon-preview-256.png` | 256×256 | 3 | false | 255 | 255 | **yes** |

Generated platform formats (`icon.icns`, size PNGs) rebuilt from the new source; not committed (gitignored per policy).

## Step 4 — Author Dock visual confirm (pending)

Per `apps/desktop/src-tauri/icons/README.md` § Cache invalidation:

1. Quit all Nexus instances
2. Rebuild/reinstall the `.app` (`pnpm dev:desktop` or release bundle)
3. `killall Dock`
4. Relaunch Nexus and inspect Dock tile

**Pass criteria:** Dock tile shows Chronos deep plate + timeline mark with **macOS squircle rounding** (not a flat square).

**Author sign-off:** Not obtained this session. Record here when confirmed.

| Outcome | Next action |
|---------|-------------|
| macOS now rounds it | Root cause confirmed: transparent alpha border defeated mask → proceed to Task 2 (docs) + Task 3 (Studio fixture) |
| Still flat square | Pin next candidate: Tauri `.icns` generation path, or deeper LaunchServices cache requiring app-bundle removal |

## RCA conclusion (provisional)

Technical rebuild satisfies the opaque full-bleed requirement. The prior transparent-inset compose strategy is the leading root-cause candidate for the flat-square Dock appearance. **Final RCA closure depends on author Dock visual confirm** after cache invalidation.
