# `apps/desktop/src-tauri/icons/`

Nexus-branded app icons for the Tauri desktop bundle.

The icons in this directory are generated from the canonical source under
`source/`:

- `source/app-icon.svg` — deterministic 1024×1024 vector composition of the
  Nexus mark on the brand deep-blue background (`#1E3A5F`). The mark uses the
  V1.83 logo geometry (`packages/nexus-ui/assets/logos/logo-primary.svg` /
  `logo-white.svg`), centered with ~10% padding so it remains visible under
  macOS squircle / Windows / Linux platform masks. Strokes are brand cyan
  (`#25D1E0`) and node circles are white (`#FFFFFF`).
- `source/source-1024.png` — rasterized 1024×1024 RGBA PNG used as the input
  for `tauri icon`. Tracked by Git LFS.
- `source/app-icon-preview-256.png` — 256×256 preview render for QA/PR review
  of the dock/taskbar appearance.

## Rasterizing the SVG source

The committed PNG was produced with [sharp](https://sharp.pixelplumbing.com/)
(Node.js). A reproducible one-liner equivalent:

```bash
cd apps/desktop/src-tauri/icons/source
node -e "
import sharp from 'sharp';
await sharp('app-icon.svg', { density: 96 })
  .resize(1024, 1024, { fit: 'contain', background: { r: 30, g: 58, b: 95, alpha: 1 } })
  .png()
  .toFile('source-1024.png');
"
```

(Actual command used: a temporary Node project with `sharp` installed via npm.)

## Regenerating all OS icon formats

From the `apps/desktop` directory:

```bash
pnpm --filter desktop exec tauri icon src-tauri/icons/source/source-1024.png
```

This overwrites, in `apps/desktop/src-tauri/icons/`:

- macOS: `icon.icns`, `32x32.png`, `128x128.png`, `128x128@2x.png`
- Windows: `icon.ico`, `Square*.png`, `StoreLogo.png`
- Linux: `icon.png`, `64x64.png`
- Mobile assets under `ios/` and `android/`

The `tauri.conf.json` `bundle.icon` array references `32x32.png`,
`128x128.png`, and `128x128@2x.png`; those paths are drop-in replacements.

## LFS policy

`source/source-1024.png` is tracked by Git LFS (binary source/provenance):

```gitattributes
apps/desktop/src-tauri/icons/source/*.png filter=lfs diff=lfs merge=lfs -text
apps/desktop/src-tauri/icons/source/app-icon-preview-256.png -filter -diff -merge
```

The 256×256 preview is intentionally kept in normal git so it can be diffed
and reviewed in GitHub/GitLab. The regenerated small-format PNGs in `icons/`
remain normal git.

## Aesthetic sign-off

User aesthetic sign-off was deferred per the V1.85 compass; the composition
above is a best-judgment, deterministic rendering of the V1.83 logo family on
the V1.84 brand palette. Review the 256×256 preview at
`source/app-icon-preview-256.png` at QA/PR time.
