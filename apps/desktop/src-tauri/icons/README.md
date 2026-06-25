# `apps/desktop/src-tauri/icons/`

App icons referenced by `tauri.conf.json` → `bundle.icon`. Generate the set with
the Tauri CLI from a single source PNG/SVG:

```bash
pnpm --filter desktop exec tauri icon path/to/source-1024.png
```

This produces `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`
(macOS), and the Windows/Linux formats.

The icon files themselves are not checked in to V1.66 P0 — they are a release
asset, not a source artifact. `tauri build` (and a complete `tauri dev` bundle)
require them; `cargo check` does not. P1/ops generates and commits them as part
of the unsigned `.app` build leg (compass §5 #10).
