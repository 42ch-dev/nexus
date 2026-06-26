# `apps/desktop/src-tauri/icons/`

App icons for the V1.66 P1 unsigned dev build.

The set in this directory is a **placeholder generated from a solid-color source**
so that CI can bundle `Nexus.app` without requiring a designer asset pipeline.
Replace the source artwork before the first public release.

To regenerate from a real source PNG/SVG:

```bash
pnpm --filter desktop exec tauri icon path/to/source-1024.png
```

This produces `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`
(macOS), and the Windows/Linux/mobile formats.

The macOS bundle references `32x32.png`, `128x128.png`, and `128x128@2x.png`
in `tauri.conf.json` → `bundle.icon`.
