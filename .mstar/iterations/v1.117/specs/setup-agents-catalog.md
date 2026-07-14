# Setup Agents catalog polish (V1.117 P1)

> Iteration-scoped product brief for V1.117 P1. Architect locked (§5.2);
> spec frozen after writing (§5.3).

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-14-v1.117-setup-agents-catalog` |
| **Tier** | Must (P1) |
| **Status** | Spec frozen (§5.3) |
| **Audience** | Authors (Setup + Settings agent picker) |
| **primary plan** | `.mstar/plans/2026-07-14-v1.117-setup-agents-catalog.md` |

## Problem framing

Setup and Settings are where authors choose their creative agent. Today the
catalog still **leaks protocol jargon** and **breaks trust**:

- Copy says "ACP" in titles and helpers (`选择 ACP 智能体`, "ACP-compatible agent").
- Install/Docs links do not reliably open the **system browser** on desktop.
- Cards ignore registry **website** and **icon** fields; static
  `setup-agent-urls.ts` is incomplete.
- Default grid still surfaces **Claude Agent / Codex ACP wrappers** instead of
  the **native** providers V1.116 made honest.
- **More Agents** does not prioritize already-installed agents.
- Install URLs need a **repo-tracked whitelist** + optional field overrides
  (no user-layer `~/.nexus42` file).

## User value

| Who | Why they care |
| --- | --- |
| **Authors (Setup)** | Plain-language agent choice; cards look like a product catalog, not a protocol debugger. |
| **Authors (desktop)** | Install/Docs open in Safari/Chrome — not a broken in-app webview. |
| **Authors (defaults)** | Claude and Codex appear as **Native Adapters** without hunting in More Agents. |
| **Maintainers** | Overrides + whitelist live in git; authors can PR new install URLs safely. |

## Goals

1. **De-ACP copy** — primary title `选择智能体` / `Choose an agent`; helpers
   describe agents in product language (no "ACP-compatible" in author-visible
   Setup/Settings picker chrome).
2. **System browser** — Install and Docs links on **desktop** invoke system
   default browser (Tauri `open_external_url` or equivalent); web build keeps
   `target=_blank` behavior.
3. **Card fields** — name, description, website (Docs when registry provides),
   icon from registry with SVG/raw URL fallback.
4. **Repo config** — `apps/web/config/agent-catalog-overrides.json` (schema v1;
   see AD-P1-1): install URL **whitelist** + optional per-agent field overrides;
   non-whitelisted install URLs hidden (not rendered as broken links).
5. **Default grid** — **Claude** / **Codex** **Native Adapters** always visible;
   hide ACP wrapper entries (`claude-acp`, `codex-acp`) from the default grid
   (may remain in More Agents if installed).
6. **More Agents** — sort **installed first**, then remainder.

## Non-goals

- User-layer `~/.nexus42` override file
- Enterprise CDN registry mirror / offline registry
- New agent providers beyond V1.116 supported inventory
- Rewriting daemon scan logic (consume existing scan response)

## Carry-forward (locked)

| Prior | What V1.117 adds |
| --- | --- |
| V1.116 detection honesty | Native Claude/Codex in scan — **surface them in default grid** |
| V1.101 `setup-agent-urls.ts` | Replace/extend with overrides config + registry fields |
| Grill-me #3 | Repo-tracked config **only** — no `~/.nexus42` user layer |

## Target state

- Setup agent step reads like a product catalog: icon, name, description,
  website, Install/Docs when allowed.
- Desktop: tapping Install/Docs leaves the app to the system browser.
- Default grid: Claude + Codex **Native Adapters** prominently; ACP wrappers deprioritized.
- More Agents: installed agents at the top.

## Acceptance criteria (author-observable)

| ID | Criterion | How to verify |
| --- | --- | --- |
| **AC-P1-1** | No "ACP" in Setup picker **title** or primary helper (en + zh-CN) | Open Setup agent step → titles/helpers are plain language |
| **AC-P1-2** | Desktop Install/Docs opens system browser | Desktop → tap Install on a whitelisted agent → external browser opens |
| **AC-P1-3** | Cards show registry name, description, icon, website when available | Visual check on Setup + Settings agent sections |
| **AC-P1-4** | Non-whitelisted install URLs are not shown as Install links | Agent without whitelist entry → no Install link (Docs may still show if allowed) |
| **AC-P1-5** | Claude + Codex **Native Adapters** in default grid | Fresh Setup → Claude and Codex Native Adapters visible without opening More Agents |
| **AC-P1-6** | ACP wrappers not in default grid | `claude-acp` / `codex-acp` not primary cards (More Agents only if installed) |
| **AC-P1-7** | More Agents lists installed agents first | Install an agent → More Agents → installed entries appear before others |
| **AC-P1-8** | Web build: links open in new tab (no regression) | Browser dev → Install uses `target=_blank` |

## Architect decisions (§5.2 — locked)

### AD-P1-1: Overrides JSON schema (v1)

File: `apps/web/config/agent-catalog-overrides.json`

```json
{
  "schema_version": 1,
  "install_whitelist": {
    "claude-acp": "https://docs.anthropic.com/en/docs/claude-code",
    "codex-acp": "https://github.com/openai/codex",
    "claude-native": "https://docs.anthropic.com/en/docs/claude-code",
    "codex-native": "https://github.com/openai/codex",
    "gemini": "https://github.com/google-gemini/gemini-cli"
  },
  "agents": {
    "claude-acp": { "hiddenFromDefault": true },
    "codex-acp": { "hiddenFromDefault": true },
    "claude-native": { "displayName": "Claude", "priority": 0 },
    "codex-native": { "displayName": "Codex", "priority": 1 }
  }
}
```

| Field | Rule |
| --- | --- |
| `install_whitelist` | Map of **agent key → https URL**. Install link renders only when key resolves and URL is present |
| `agents.<key>` | Optional partial override: `displayName`, `docsUrl`, `installUrl` (must still pass whitelist), `iconUrl`, `hiddenFromDefault`, `priority` (lower = earlier in default grid) |
| **Agent key resolution** | `registry_agent_id` when set; else native provider id (`claude-native`, `codex-native`) derived from scan merge; else collision-safe picker id |

Replace `setup-agent-urls.ts` with loader that merges: **scan entry** → **overrides** → **registry `icon_url` / `description`**.

Validate at build time (TypeScript type + unit test); no runtime JSON Schema dep.

### AD-P1-2: `open_external_url` (desktop)

| Layer | Contract |
| --- | --- |
| Rust | `open_external_url(url: String) -> Result<(), String>` — allow only `http:` / `https:`; call `opener::open(url)` (same engine as `plugin-opener`; **no** workspace path guard) |
| JS | `DesktopCapabilities.openExternalUrl(url: string)` on `TauriDesktopCapabilities` |
| AgentPicker | Desktop: `onClick` → `openExternalUrl`; browser: keep `<a target="_blank" rel="noopener">` |

Register command in `lib.rs` + `capabilities/main.json`. On failure: host shows toast
(`common.errors.openExternalFailed` or equivalent) — do not silently no-op.

**Do not** use `open_with` for URLs — that command is path-guarded for workspace files.

### AD-P1-3: Icon pipeline

1. Prefer scan/registry `icon_url` when present (remote SVG/PNG).
2. On load error → static fallback from `agents.<key>.iconUrl` in overrides.
3. No new bundler pipeline — optional bundled assets only in overrides config.

### AD-P1-4: Native id mapping (default grid)

| Product card | Canonical override key | Scan signal |
| --- | --- | --- |
| Claude (Native Adapter) | `claude-native` | `registry_agent_id: null` + `launch_command: "claude"` |
| Codex (Native Adapter) | `codex-native` | `registry_agent_id: null` + `launch_command: "codex"` |
| Claude ACP wrapper | `claude-acp` | `hiddenFromDefault: true` — More Agents only if installed |
| Codex ACP wrapper | `codex-acp` | `hiddenFromDefault: true` |

Default grid = installed agents matching native keys **or** `priority` listed in
overrides, excluding `hiddenFromDefault`. Sort: `priority` asc, then installed-first
within More Agents (product AC-P1-7).

Saved profile (`set_agent_profile`) continues to store provider **id** string
(e.g. `codex-native` or `codex-acp`) — align picker selection id with override key
where possible (may require mapping native scan rows to `*-native` keys in host).

## Key files (expected)

- `apps/web/src/components/setup/agent-picker.tsx`
- `apps/web/src/pages/setup-agent-urls.ts` → replace with overrides loader
- `apps/web/src/pages/setup-step-agent.tsx`, `settings-agent-section.tsx`
- `apps/web/config/agent-catalog-overrides.json` (new)
- Desktop opener capability in `apps/desktop/src-tauri`
- Locales: `setup.json`, `settings.json`
- Registry FORMAT: https://github.com/agentclientprotocol/registry/blob/main/FORMAT.md
