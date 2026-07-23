# P0 — Dock icon squircle follow-up (V1.136)

**Status:** draft (Phase 1 §1.6 — **Architect §5.2 technical contract locked**; writing-specialist §5.3 complete — PM lock next)  
**Plan:** `2026-07-23-v1.136-p0-dock-icon-squircle-followup`  
**Carry:** `R-V1135P1-001`, `R-V1135P1-005` (closes `R-V1134P1-001` when P0G-1 passes)

## Author problem (plain language)

When I pin **Nexus** to the macOS Dock, the icon still looks like a **sharp square** — not the rounded squircle every other native app uses. V1.135 tried baking squircle margins into the compose pipeline (H6), but on my machine it still reads square. I need the **real** fix (contrast, cache, wrong binary, or template) — not another preview PNG that looks fine in Studio.

**【图1】 evidence:** Sharp-square Dock tile for `nexus-desktop`.

## User value

- **Trust:** Desktop app looks like a first-class macOS citizen in the Dock.
- **Clarity:** Author and QC share one Done definition — live Dock eyeball after a documented ritual, not asset inspection alone.

## Intent

Author must see a **macOS squircle** Dock tile for `nexus-desktop`, not a sharp square. V1.135 H6 bake was insufficient or invisible; continue ordered RCA until primary cause is fixed or next candidate is named with evidence.

## Product acceptance (P0G — PM locked)

Author-observable gates. Plan AC-1–AC-4 map 1:1 below.

| ID | Author can observe… | Pass | Fail |
|----|---------------------|------|------|
| **P0G-1** | Dock tile shape | Rounded **macOS squircle** on live Dock after documented quit → rebuild → `killall Dock` → relaunch ritual | Sharp full square; agent claims Done from PNG/Studio/icns alone |
| **P0G-2** | Correct app under test | Running `.app` matches documented build output; `CFBundleIconFile` / icns attached | Stale duplicate bundle, dev bundle, or wrong product name |
| **P0G-3** | RCA transparency | Written primary cause **or** next named hypothesis with evidence (margin contrast, LS cache, template, binary path) | Opaque “tweaked compose” with no written cause |
| **P0G-4** | Honest residual | Residual stays **open** until P0G-1 passes; no fake Done | Closing `R-V1135P1-001` without `@author` eyeball |

**Compass mapping:** AC-I1 ↔ P0G-1, P0G-3, P0G-4.

## Anti-patterns (do not ship)

1. **Studio/PNG Done** — VI preview or opaque PNG inspection without Dock proof.
2. **Same-color margin “fix”** — `#0D2B3E` plate + margin invisible to author; must test contrast ≠ plate or alternate bake strategy.
3. **Closing residual without author** — `R-V1135P1-001` / `R-V1135P1-005` require `@author` for visual gate.
4. **Unrelated desktop chrome** — scope creep into shell/window theming.

## Residual disposition

| Residual | Disposition |
|----------|-------------|
| `R-V1135P1-001` | Close when **P0G-1** passes (`@author`) |
| `R-V1135P1-005` | Close when H6 subtlety resolved or superseded by named next candidate + author confirm |
| `R-V1134P1-001` | Closes with `R-V1135P1-001` |

## Non-goals

- Treating Studio preview as Dock Done
- Unrelated desktop chrome or web favicon work
- Knowledge edits in Phase 1 (implement extends `guides/p0-dock-icon-rca.md`; carry forward from V1.135 `guides/p1-dock-icon-rca.md`)

## Architect technical contract (§5.2 — normative)

### File ownership

| Owner | Path | Role |
|-------|------|------|
| **Pipeline SSOT** | `apps/desktop/src-tauri/icons/source/compose-app-icon.mjs` | Squircle bake geometry, plate contrast, raster output |
| **Generate entry** | `apps/desktop/package.json` (`icons:compose`, `icons:generate`) | Reproducible iconset + `.icns` |
| **Dev ritual** | Root `package.json` `dev:desktop` | Must run `icons:generate` before `exec tauri dev` (H7 — retain) |
| **Bundle wiring** | `apps/desktop/src-tauri/tauri.conf.json` `bundle.icon[]` | Points at generated PNG members |
| **Author docs** | `apps/desktop/src-tauri/icons/README.md` | Rebuild + Dock ritual (update when pipeline changes) |
| **RCA log (implement)** | `.mstar/iterations/v1.136/guides/p0-dock-icon-rca.md` | Extend V1.135 guide — **not** `{KNOWLEDGE_DIR}/` |

### Ordered RCA ladder (H1 → H8+)

Carry V1.135 falsifications. V1.136 **must** re-run and record evidence for **H4, H5, H6** before claiming a new fix.

| ID | Hypothesis | V1.136 action |
|----|------------|---------------|
| **H4** | Wrong/stale `.app` under test | **Mandatory** — record `mdfind`, bundle path, `CFBundleIdentifier`, process name, build command |
| **H5** | LaunchServices cache beyond `killall Dock` | **Mandatory** — document ritual completion; if fail persists, run `lsregister` reset per README extended steps |
| **H6** | Plate geometry / margin contrast invisible | **Primary** — if re-bake, margin color **must** contrast with `#0D2B3E` plate OR use alternate strategy (lighter rim, inner highlight, template export) |
| **H7** | Dev path skipped `icons:generate` | Verify still wired — do not regress |
| **H8+** | Asset catalog / template / `@2x` scaling | Name next candidate only after H4–H6 evidence recorded |

### H4 / H5 / H6 evidence template (required per attempt)

Implementers append one row block per RCA attempt to `guides/p0-dock-icon-rca.md`:

```markdown
### Attempt <n> — <YYYY-MM-DD>

| Field | Value |
|-------|-------|
| **Hypothesis** | H4 / H5 / H6 / H8+ |
| **Build command** | _exact command_ |
| **Bundle path** | _absolute path to Nexus.app under test_ |
| **CFBundleIdentifier** | _from Info.plist_ |
| **mdfind result** | _output of `mdfind "kMDItemCFBundleIdentifier == 'io.nexus42.desktop'"`_ |
| **H4 duplicate check** | _single bundle yes/no; stale install notes_ |
| **Cache ritual** | quit all → icons:generate → rebuild → killall Dock → relaunch — _yes/no_ |
| **H5 extended** | _lsregister reset yes/no if applied_ |
| **H6 geometry** | _corner RGB samples; margin≠plate yes/no; compose params changed_ |
| **Author outcome** | _Pending / Pass squircle / Fail still square_ |
| **Next candidate** | _if Fail — named H8+ only after table complete_ |
```

**Anti-pattern:** Tweaking compose without filling the template — fails **P0G-3**.

### Verify ritual (author gate — normative)

1. Quit **all** `nexus-desktop` / Nexus instances.
2. `pnpm --filter desktop run icons:generate`
3. Rebuild/reinstall documented `.app` (`pnpm dev:desktop` **or** release `pnpm --filter desktop run build`).
4. `killall Dock` (macOS relaunches Dock).
5. Relaunch Nexus; inspect Dock at normal size.
6. **Pass (P0G-1):** visible macOS squircle on outer tile. **Fail:** residual stays open; next candidate named in RCA guide.

### Test / proof contract

| Proof type | Allowed for Done? |
|------------|-------------------|
| Live macOS Dock eyeball (`@author`) | **Yes** — only closure path |
| Studio VI-004 / CSS squircle preview | **No** |
| PNG / icns / opacity metadata alone | **No** |
| Agent assertion without author row | **No** |

## PM sign-off (§5.1)

| Field | Value |
|-------|-------|
| **Product intent** | Ready for Architect §5.2 |
| **Date** | 2026-07-23 |
| **Blocked** | None — clarify closed via author screenshots + V1.135 carry-forward |

## Architect sign-off (§5.2)

| Field | Value |
|-------|-------|
| **Technical contract** | Locked — pipeline ownership, H4/H5/H6 evidence template, verify ritual |
| **Date** | 2026-07-23 |
| **PM Q4** | H4/H5/H6 RCA evidence template extended above |
| **Deferred to implement** | `guides/p0-dock-icon-rca.md` creation; H8+ only after template rows |
