# Profiles + per-Profile workspace (V1.117 P0)

> Iteration-scoped product brief for V1.117 P0. Not a normative `{SPECS_DIR}`
> master. Architect locked (§5.2); spec frozen after writing (§5.3).

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-14-v1.117-profiles-workspace` |
| **Tier** | Must (P0) |
| **Status** | Spec frozen (§5.3) |
| **Audience** | Authors (first launch + ongoing Settings) + maintainers (config migration) |
| **primary plan** | `.mstar/plans/2026-07-14-v1.117-profiles-workspace.md` |

## Problem framing

Authors think in **Profiles** (creator identity), not anonymous IDs or a generic
"Workspace" tab. Today:

- Setup bootstrap writes `active_creator_id` without a friendly **Profile name**
  field on the Setup Profile step.
- Settings labels the section **工作区 / Workspace** while the sidebar footer
  already says **Profiles**.
- `workspace_path` in `~/.nexus42/config.toml` is **global** — switching
  Profile in the footer does not switch the creative root folder.

V1.104 shipped editable workspace path from Settings (single global path +
restart honesty). V1.117 **promotes Profile identity** and makes workspace path
**per-Profile** (P0).

## User value

| Who | Why they care |
| --- | --- |
| **Authors (first launch)** | Setup explains a Profile is created; they can name it before entering the app. |
| **Authors (multi-Profile)** | Each Profile can own a workspace folder; switching Profile switches the active creative root. |
| **Authors (returning)** | Settings **Profiles** tab matches footer mental model; no jargon mismatch between Workspace and Profiles. |
| **Maintainers** | Migration from legacy single `workspace_path` is explicit; tests cover bootstrap + switch. |

## Goals

1. **Default Profile guarantee** — when the UI is open, at least one Profile
   exists (bootstrap on Setup if missing). The first Profile is the author's
   **Default Profile** (display name editable; internal `creator_id` unchanged).
2. **Setup Profile step** — copy explains a Profile is created; field to edit
   **Profile display name** (not only folder path). Component may retain
   `setup-step-workspace` filename until refactor.
3. **Settings rename** — tab/section **工作区 → Profiles** (en: Workspace →
   Profiles); route stays `/settings/workspace` (label-only; AD-P0-4).
4. **P0: per-Profile workspace path** — each Profile stores its workspace root
   path; **switching active Profile** persists/activates that Profile's path
   (same honesty pattern as V1.104: restart/reload may be required for full
   effect).
5. **Profiles settings** — edit display name + workspace path for the **active**
   Profile.

## Non-goals

- Extra Profile metadata (avatar, theme, cloud sync)
- Automatic file migration between workspace folders when path changes
- Live workspace-root refresh without restart (V1.66/V1.67+ scope; carry V1.104
  honesty copy)
- Renaming internal `creator_id` values

## Carry-forward (locked)

| Prior | What V1.117 adds |
| --- | --- |
| V1.100 bootstrap | `active_creator_id` + `active_workspace_slug_by_creator` — keep; add per-Profile **path** map |
| V1.104 Settings workspace | Editable path + honesty banner — **per-Profile** instead of global only |
| Footer Profiles (existing) | Switching Profile must **activate that Profile's workspace path** (P2 shell depends on P0 T4) |

## Migration (product)

Existing installs with a single global `workspace_path`:

1. On first read after upgrade, copy legacy `workspace_path` into the **active /
   Default Profile's** `[workspace_path_by_creator]` entry when that key is
   absent (one-shot dual-read; AD-P0-2).
2. Continue dual-write: per-creator map is SSOT; mirror the **active** Profile's
   path to top-level `workspace_path` so CLI/daemon/sidecar keep working without
   a daemon-runtime change in V1.117.
3. If path changes require restart, show the same **honest** copy as V1.104
   (`settings-workspace-saved-honesty` pattern) — forbidden: "instant everywhere".

## Target state

- Fresh Setup: author names their Profile; workspace step reads as Profile +
  folder, not anonymous bootstrap.
- Settings → **Profiles**: manage name + path for the Profile(s).
- Footer Profile switch: active Profile changes **and** workspace root follows
  (with restart honesty when applicable).
- Browser build: honest desktop-only disabled state for path pickers (mirror
  V1.104).

## Acceptance criteria (author-observable)

| ID | Criterion | How to verify |
| --- | --- | --- |
| **AC-P0-1** | Default Profile always exists after Setup or first open | Complete Setup or open app → footer shows at least one Profile |
| **AC-P0-2** | Setup Profile step includes editable **Profile name** + explains Profile creation | Setup copy mentions Profile; name field persists to display name |
| **AC-P0-3** | Settings section labeled **Profiles** (en + zh-CN) | Settings nav shows Profiles, not Workspace-only label |
| **AC-P0-4** | Workspace path is **per-Profile** | Set path on Profile A → switch to Profile B → path field reflects B's path (or default) |
| **AC-P0-5** | Footer switch activates switched Profile's workspace path | Switch Profile → persisted path matches selection (desktop); honesty copy if restart needed |
| **AC-P0-6** | Legacy single `workspace_path` users keep their folder on Default Profile | Upgrade config with only global path → Default Profile shows that path |
| **AC-P0-7** | Browser: no fake path API | Browser Settings shows desktop-only honesty for path change |

## Architect decisions (§5.2 — locked)

### AD-P0-1: Config shape

Add a TOML table keyed by `creator_id`, parallel to the existing bootstrap map:

```toml
active_creator_id = "ctr_local…"
workspace_path = "/Users/author/Documents/nexus"   # legacy mirror — active Profile only

[workspace_path_by_creator]
ctr_local… = "/Users/author/Documents/nexus"

[active_workspace_slug_by_creator]
ctr_local… = "default"
```

- **SSOT for UI:** `[workspace_path_by_creator]`.
- **Legacy mirror:** top-level `workspace_path` always reflects the **active**
  Profile's path after any write (Settings save or footer switch).
- **Read order:** `workspace_path_by_creator[active_creator_id]` → fallback
  `workspace_path` → default home path (`~/Documents/nexus`).

No JSON Schema / wire contract change — `~/.nexus42/config.toml` only.

### AD-P0-2: Dual-read / migration

| Phase | Behavior |
| --- | --- |
| **First read after upgrade** | If `[workspace_path_by_creator]` missing/empty and legacy `workspace_path` set → copy into active (or Default) `creator_id` entry and persist |
| **Steady state** | Reads prefer per-creator map; legacy key kept in sync on write |
| **Deprecation** | Do **not** remove `workspace_path` in V1.117 — CLI + sidecar still read it |

### AD-P0-3: Tauri commands

| Command | V1.117 contract |
| --- | --- |
| `set_workspace_path(path)` | Write `path` to `[workspace_path_by_creator][active_creator_id]` **and** mirror to `workspace_path` |
| `get_workspace_root()` | Return active Profile's resolved path (map → legacy → default) |
| Profile switch (footer) | Update `active_creator_id`, then set legacy `workspace_path` to switched Profile's map entry (create default entry if missing) |

**No new `creator_id` parameter** on `set_workspace_path` — infer from config
`active_creator_id` (matches V1.100 bootstrap). Footer switch must call switch +
path mirror atomically in one Tauri command or sequenced invoke (implementer
choice; outcome must match AC-P0-5).

**Startup cache (V1.66):** path guard + sidecar workspace root still captured at
app launch — carry V1.104 honesty banner; live refresh remains out of scope.

### AD-P0-4: Route

Keep `/settings/workspace` and `settings-workspace-section` test ids; change
**nav label + page title** to Profiles only. Avoid route churn in `App.tsx` /
`settings-shell-layout.tsx` paths.

### AD-P0-5: Profile display name

Use existing daemon `display_name` on `CreatorDetail` / `PATCH` creator — no new
config.toml field. Setup + Settings edit `display_name` via existing creator API.

### AD-P0-6: Switch timing

| Event | Action |
| --- | --- |
| Settings open | Load path for **active** Profile from map |
| Settings save | Write map + legacy mirror for active Profile |
| Footer switch | Set `active_creator_id` + legacy `workspace_path` to target Profile's map entry |
| App restart | Sidecar reads mirrored `workspace_path` (unchanged daemon contract) |

## Key files (expected)

- `apps/desktop/src-tauri/src/lib.rs` (config.toml bootstrap / workspace path APIs)
- `apps/web/src/pages/setup-step-workspace.tsx`
- `apps/web/src/pages/settings/settings-workspace-section.tsx` → Profiles
- `apps/web/src/components/layout/footer-profiles.tsx`
- Locale: `settings.json`, `setup.json`, `shell.json`
