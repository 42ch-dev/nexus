# Mac Dock transparent icon (V1.117 P3)

> Iteration-scoped product brief for V1.117 P3. Architect locked (§5.2);
> spec frozen after writing (§5.3).

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-14-v1.117-mac-dock-icon` |
| **Tier** | Should (P3) — recommended for iteration close; not a Must blocker |
| **Status** | Spec frozen (§5.3) |
| **Audience** | Desktop authors (macOS Dock first impression) |
| **primary plan** | `.mstar/plans/2026-07-14-v1.117-mac-dock-icon.md` |

## Problem framing

On macOS the Dock shows Nexus as a **sharp white/off-white square** instead of
a logo on the system squircle. Root cause: `compose-app-icon.mjs` fills the
1024×1024 canvas with opaque `DESIGN.md` `background-200` (`#fafafa`). macOS
already applies squircle masking — the asset should use a **transparent**
canvas and let the OS chrome provide the plate.

## User value

| Who | Why they care |
| --- | --- |
| **Desktop authors** | Dock icon matches other native apps — logo on system shape, not a pasted white tile. |
| **Maintainers** | One compose script change + regen pipeline; README matches behavior. |

## Goals

1. Compose pipeline uses **transparent** canvas background (`alpha: 0`); keep
   existing logo artwork and **soft shadow at reduced opacity** (AD-P3-1).
2. Regenerate `source-1024.png` / preview + full `icons:generate` / `tauri icon`
   pipeline outputs.
3. Update `apps/desktop/src-tauri/icons/README.md` — remove claims of white /
   `background-200` full-bleed fill.

## Non-goals

- Hand-drawn baked squircle inside the PNG
- Windows/Linux icon redesign beyond shared pipeline output
- Marketing icon variants / App Store assets

## Target state

- macOS Dock: Nexus logo on system squircle — **no opaque full-bleed square**.
- README and compose log message describe transparent background.

## Acceptance criteria (author-observable)

| ID | Criterion | How to verify |
| --- | --- | --- |
| **AC-P3-1** | Compose script uses transparent canvas | Read `compose-app-icon.mjs` → `background.alpha: 0` (or equivalent) for canvas |
| **AC-P3-2** | Regenerated icons built in predev/build pipeline | `pnpm icons:generate` (or project script) succeeds; Tauri build picks up assets |
| **AC-P3-3** | Dock visual smoke — no white square | macOS: install/run desktop app → Dock icon shows logo without full-bleed white plate |
| **AC-P3-4** | README accurate | Icons README does not instruct white/`background-200` fill for Dock asset |

## Architect decisions (§5.2 — locked)

### AD-P3-1: Shadow on transparent canvas

**Keep** the existing `brand-deep-blue-1000` drop shadow under the mark:

| Parameter | V1.117 value | Rationale |
| --- | --- | --- |
| Canvas background | `alpha: 0` (fully transparent) | macOS applies squircle mask — opaque fill caused white square |
| Shadow opacity | `0.12` (was `0.15`) | Maintains depth on varied Dock wallpapers without muddy small-size icon |
| Mark | Unchanged `logo_light.png` trim + 10% padding | — |

Remove `background-200` fill from `compose-app-icon.mjs`; update console log and README.

### AD-P3-2: Regen / commit policy

| Asset | Policy |
| --- | --- |
| `source/source-1024.png`, `app-icon-preview-256.png` | Regenerate via `pnpm --filter desktop run icons:compose`; **commit** when visually verified (Git LFS) |
| Platform outputs (`icon.icns`, etc.) | **Gitignored** — built by `predev` / `icons:generate` |
| QA | macOS Dock visual smoke required for iteration close (AC-P3-3) |

## Key files (expected)

- `apps/desktop/src-tauri/icons/source/compose-app-icon.mjs`
- `apps/desktop/src-tauri/icons/README.md`
- Generated icons (gitignored; built on predev/build)
