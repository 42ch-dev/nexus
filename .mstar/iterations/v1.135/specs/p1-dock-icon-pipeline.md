# P1 — Dock icon pipeline (normative contract)

> **Status:** Normative (Architect Review & Edit §5.2, 2026-07-23)  
> **Document class:** Iteration Draft overlay — V1.135 P1  
> **Coordinates with:** [`delivery-compass.md`](../delivery-compass.md); plan [`2026-07-23-v1.135-p1-dock-icon-squircle-rca`](../../../plans/2026-07-23-v1.135-p1-dock-icon-squircle-rca.md)  
> **Product verify gate:** P1G-1–P1G-5 (PM-locked, below).  
> **Carries forward:** `R-V1134P1-001` — open until P1G-1 author Dock squircle confirm or plan Blocked with exhausted candidates.

## Author problem (plain language)

The macOS Dock tile for **nexus-desktop** still looks like a **sharp square**, not a rounded squircle like normal Mac apps — after ~6 iterations including V1.134’s opaque full-bleed PNG fix.

**【图2】 evidence:** Dock screenshot post–V1.134 opaque compose — still square.

---

## Architecture decision record (locked)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Done surface** | Live macOS Dock tile for the **running** `.app` under test | Studio VI-004, `app-icon-preview-256.png`, or PNG opacity metadata alone |
| **Source SSOT** | `apps/desktop/src-tauri/icons/source/` + `compose-app-icon.mjs` | Reproducible; LFS tracks `source-1024.png` |
| **Generation path** | `pnpm --filter desktop run icons:generate` | compose → `tauri icon` → platform outputs |
| **Bundle wiring** | `tauri.conf.json` → `bundle.icon[]` + Tauri-generated `Info.plist` → `CFBundleIconFile` | Verified: release bundle uses `Nexus.icns` |
| **V1.134 opacity fix** | Retain as **baseline H1** — necessary, not sufficient | Author still sees square; RCA continued |
| **V1.135 H6 compose fix** | Bake pre-rounded squircle plate on opaque canvas (Task 2) | Addresses square-plate geometry; **P1G-1 author Dock confirm still required** |
| **Wire contracts** | `wire_contracts_changed: false` | Desktop asset pipeline only |

---

## Pipeline interface (normative)

### Stage map

```
logo-primary-square.svg  (@42ch/nexus-ui assets)
        │
        ▼  icons:compose (compose-app-icon.mjs — baked squircle plate)
source-1024.png ─────────────► app-icon-preview-256.png (QA preview, git)
        │
        ▼  tauri icon source-1024.png  (icons:generate)
icon.icns, 32x32.png, 128x128.png, 128x128@2x.png, …
        │
        ▼  tauri build / tauri dev (predev + beforeBuildCommand)
Nexus.app/Contents/Resources/Nexus.icns
Info.plist CFBundleIconFile = Nexus.icns
        │
        ▼  macOS Dock (LaunchServices)
Author-visible squircle tile  ◄── P1G-1 gate
```

### Script contracts

| Script | Entry | Outputs | Invariants |
|--------|-------|---------|------------|
| `icons:compose` | `src-tauri/icons/source/compose-app-icon.mjs` | `source/source-1024.png`, `source/app-icon-preview-256.png` | 1024×1024 opaque RGB; `hasAlpha: false`; **pre-rounded squircle plate** centered on canvas (6% opaque `#0D2B3E` margin, 22% corner radius on inner plate — matches Studio VI-004); **no transparent margins** |
| `icons:generate` | `apps/desktop/package.json` | `icon.icns`, size PNGs under `src-tauri/icons/` | Runs compose first; deletes `ios/`, `android/` |
| `predev` / `beforeBuildCommand` | `tauri.conf.json` | Regenerates icons before dev/build | Fresh clone must not require committed generated binaries |
| `dev:desktop` (root) | `package.json` | Runs `icons:generate` before `exec tauri dev` | H7 fix — `exec tauri dev` bypasses `predev`; root script wires generate explicitly |

### Config contracts

| Field | Path | Value / rule |
|-------|------|--------------|
| `bundle.icon` | `tauri.conf.json` | `["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png"]` |
| `CFBundleIconFile` | Built `Nexus.app/Contents/Info.plist` | Must reference existing `.icns` in `Contents/Resources/` |
| `productName` / executable | `tauri.conf.json` | `Nexus` / `nexus-desktop` — Dock tooltip identity check (P1G-2) |
| `identifier` | `tauri.conf.json` | `io.nexus42.desktop` — duplicate-bundle detection |

### Evidence artifacts (required)

| Artifact | Owner | Purpose |
|----------|-------|---------|
| `.mstar/iterations/v1.135/guides/p1-dock-icon-rca.md` | Implementer Task 1 | Multi-hypothesis RCA + primary root cause + author confirm block |
| `apps/desktop/src-tauri/icons/README.md` | Task 2/3 | Verify ritual + cache invalidation (author-facing) |
| `apps/desktop/AGENTS.md` | Task 3 | Dock-done ≠ opacity-only (durable guard) |

**Do not commit** generated `icon.icns` / size PNGs if gitignored — regenerate in dev/CI.

---

## Ordered hypothesis list (falsifiable — implement in this order)

Stop at first **confirmed** primary cause that explains author square **and** passes P1G-1 after fix. If H1–H6 exhausted without Dock pass → plan **Blocked**, name H7+ in RCA.

| Order | ID | Hypothesis | Falsifiable checks | If confirmed → fix direction |
|-------|-----|------------|-------------------|------------------------------|
| **1** | **H1** | Transparent / partial-alpha source defeated squircle mask (V1.134) | `sharp` metadata on `source-1024.png`: `hasAlpha: false`, channels=3, minAlpha=255; compare pre/post V1.134 inset compose | Already applied — **retain**; if metadata fails, re-run compose |
| **2** | **H2** | Generated `.icns` / iconset defective (alpha, missing sizes, stale file) | After `icons:generate`: `iconutil -c iconset /tmp/nexus.iconset icon.icns` OR `sips -g all` on extracted members; verify 16–512@2x set; compare mtime vs `source-1024.png` | Regenerate; fix `tauri icon` inputs; ensure hook runs on dev path |
| **3** | **H3** | Bundle does not ship regenerated icon (wrong path, stale `Resources/`) | Inspect built `.app/Contents/Resources/Nexus.icns` hash/mtime vs `src-tauri/icons/icon.icns`; `plutil -p Info.plist` for `CFBundleIconFile` | Fix `bundle.icon` / build hooks; ensure `beforeBuildCommand` runs for author test path |
| **4** | **H4** | Dock shows **wrong binary** (stale install, duplicate bundle, dev vs release) | Document author test command (`pnpm dev:desktop` vs `tauri build`); `ps aux \| grep -i nexus`; `lsappinfo` / Get Info on running app path; check for second `io.nexus42.desktop` copy | Author reinstall from documented path; remove duplicate `.app` |
| **5** | **H5** | LaunchServices cache beyond `killall Dock` | After H4 clean install: quit all instances → rebuild → `killall Dock` → relaunch; if fail: remove old `.app` + empty Trash → `touch` bundle → repeat | Document extended cache ritual in README; escalate residual `@author` |
| **6** | **H6** | Source geometry reads as square plate under mask (full-bleed square `#0D2B3E` to edges) vs true mask failure | Compare Dock peer apps; overlay macOS squircle template on `source-1024.png`; if mask works in Preview but Dock square → points to H2–H5 not geometry | **Fixed (Task 2):** bake visible squircle rounding into compose — pre-rounded plate on opaque canvas (6% inset, 22% radius) |
| **7** | **H7** | `tauri dev` / debug bundle uses different icon path than release | Compare `target/debug/bundle` vs `target/release/bundle` Resources; verify `predev` ran | **Fixed (Task 2):** root `pnpm dev:desktop` runs `icons:generate` before `exec tauri dev` |

**Process rule:** Do not close plan on H1 alone. P1G-4 requires written RCA with **primary** cause selected from falsified/confirmed set.

---

## Product verify gate (author AC — PM locked)

| ID | Gate | Pass | Fail |
|----|------|------|------|
| **P1G-1** | Dock shape | Tile shows **squircle** mask (rounded icon plate like peer apps) | Sharp square / rectangular plate |
| **P1G-2** | Dock identity | Tooltip/process matches **Nexus desktop** build under test | Wrong bundle, dev vs installed mismatch undocumented |
| **P1G-3** | Verify ritual | Author followed documented cache-invalid steps before judging | Judged from old Dock cache or wrong binary |
| **P1G-4** | RCA depth | Written multi-hypothesis RCA with **primary root cause** selected | Single-line compose change with no Dock proof |
| **P1G-5** | Plan closure | **Author confirm** in `guides/p1-dock-icon-rca.md` **or** plan **Blocked** with next candidate | Done on Studio/PNG evidence only |

### Author Dock confirm checklist (mandatory for P1G-1 / P1G-5)

1. Quit **all** Nexus / `nexus-desktop` instances.
2. Rebuild or reinstall the **`.app`** under test — **record exact command** in RCA (e.g. `pnpm dev:desktop` dist-load vs `pnpm --filter desktop run build`).
3. `killall Dock`
4. Relaunch Nexus; inspect Dock tile at normal size.
5. **Pass:** squircle rounding visible → record in RCA; close `R-V1134P1-001`. **Fail:** continue RCA; plan open or Blocked.

---

## Anti-patterns (do not ship)

1. **Approve on Studio VI-004 alone** — supporting evidence only.
2. **Approve on opacity metadata** — `hasAlpha: false` does not prove Dock squircle.
3. **Opacity-only compose tweak** without Dock confirm — insufficient unless H2–H7 falsified **and** author confirms.
4. **Silent Done** — author unavailable → residual `@author`; do not fake P1G-1.

## V1.134 residual carry-forward

| Residual | Disposition |
|----------|-------------|
| `R-V1134P1-001` | Close when **P1G-1** author Dock confirm recorded |
| `R-V1134P1-002` (Studio VI-004) | Supporting only |
| V1.134 P1 `DONE_WITH_CONCERNS` | Superseded — deep RCA required |

## Non-goals

- Sidebar create IA (P0)
- iOS / Android icon polish
- Code signing / notarization
- Committing large generated binaries

## Architect sign-off

| Field | Value |
|-------|-------|
| **Signed** | Architect §5.2 Review & Edit |
| **Date** | 2026-07-23 |
| **Hypothesis order** | H1→H7 as above; Dock confirm hard gate |
| **Prior RCA** | `.mstar/iterations/v1.134/guides/p1-app-icon-rca.md` — H1 baseline, author confirm never obtained |
