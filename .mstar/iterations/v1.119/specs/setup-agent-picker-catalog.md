# P2 Spec — AgentPicker catalog polish

**Status:** Draft (Phase 1 — product §5.1, architect §5.2, writing §5.3 locked)  
**Document class:** Draft overlay  
**Iteration compass:** [delivery-compass.md](../delivery-compass.md)  
**Surfaces:** Setup Agent step + Settings Agent section (shared `AgentPicker`)

## Problem statement

Setup Agent cards show placeholder icons for native Codex/Claude, still surface Install/Docs on installed agents, list ACP wrapper dupes under More, and lack a curated default grid matching author-facing order.

## Target users

| Persona | Scenario |
| --- | --- |
| Author with Claude/Codex installed | Sees recognizable ACP icons on native cards; no Install/Docs clutter |
| Author browsing agents | Predictable top grid (12 curated slots) + More for the rest |
| Author in Settings | Same catalog behavior as Setup |

## User stories

1. **As an author**, installed agents show name, Installed badge, version, and description only — no Install or Docs links.
2. **As an author**, uninstalled agents in the curated grid still show Install (and Docs when configured).
3. **As an author**, I never see duplicate Claude/Codex ACP wrapper cards (`claude-acp`, `codex-acp`) in default grid or More.
4. **As an author**, the default grid order matches the curated product list, with installed agents listed before uninstalled within each tier.

## Product rules (normative)

### Icons (F1)

- `codex-native` and `claude-native` **must** use the same `iconUrl` as the ACP registry entries for `codex-acp` and `claude-acp` respectively (via [`agent-catalog-overrides.json`](../../../../apps/web/config/agent-catalog-overrides.json) `iconUrl` override — no runtime registry fetch required in picker).
- Fallback: generic user icon (current behavior) only when URL fails to load.

### Hard exclude (F2)

- `codex-acp` and `claude-acp` **never** appear in default grid **or** More list (exclude at catalog filter layer, not merely `hiddenFromDefault`).
- Other `*-acp` agents (e.g. `pi-acp`, `junie-acp`) **may** appear in More when not curated — **only** Claude/Codex wrappers are hard-excluded.

### Installed chrome (F3)

| State | Card shows | Card hides |
| --- | --- | --- |
| **Installed** | displayName, Installed badge, version (if scan provides), description (if scan provides) | Install, Docs |
| **Uninstalled** | displayName, Not installed badge, Install (when whitelist URL exists), Docs (when override configured) | — |

Selection affordance unchanged (installed selectable; uninstalled discoverability-only).

### Curated default order (F3)

Display order for **uninstalled** priority slots (catalog keys):

| # | Display name | Catalog key (primary) | Registry / fallback match |
| --- | --- | --- | --- |
| 1 | Codex | `codex-native` | native launch map |
| 2 | Claude | `claude-native` | native launch map |
| 3 | Cursor | `cursor` | registry id |
| 4 | Kimi | `kimi` | registry id |
| 5 | OpenCode | `opencode` | registry id |
| 6 | Hermes | `hermes` | registry id **or** name contains `hermes` (forward-compat) |
| 7 | Gemini | `gemini` | registry id |
| 8 | Grok Build | `grok-build` | registry id **or** name contains `grok` (forward-compat) |
| 9 | Kilo | `kilo` | registry id **or** name contains `kilo` (forward-compat) |
| 10 | Pi | `pi-acp` | registry id (display **Pi**) |
| 11 | Qoder | `qoder` | registry id |
| 12 | Qwen Code | `qwen-code` | registry id |

Slots with no matching scan row are **skipped** (no empty placeholder cards). Architect assigns `priority` 0–11 in overrides for keys that exist.

### Sort / partition (F3)

1. **Default grid:** all **installed** agents first (stable order: curated priority asc, then name asc), then **curated uninstalled** in table order above, then any other installed-not-in-curated (priority undefined, name asc).
2. **More:** remaining agents excluding default-grid ids and hard-excluded wrappers; sort installed-first, then name asc.
3. Settings Agent section uses the **same** `defaultGridEntries` / `moreAgentsEntries` pipeline as Setup (no forked sort).

### Version / description gaps

When scan returns null version/description for native agents: show card without those lines; **do not** block ship on wire changes. Optional static `description` in overrides when scan metadata is null — **residual OK** if omitted at ship.

**Recommended override copy (EN, §5.3 locked):**

| Key | `description` |
| --- | --- |
| `claude-native` | Anthropic's agent for local coding with Claude. |
| `codex-native` | OpenAI's agent for local coding with Codex. |

Overrides are EN-only product strings (not `setup` locale files). Implementer may omit if scan supplies description.

## Scope boundary

| In scope | Out of scope |
| --- | --- |
| [`agent-picker.tsx`](../../../../apps/web/src/components/setup/agent-picker.tsx) Install/Docs gating | Workspace Continue (P0) |
| [`agent-catalog.ts`](../../../../apps/web/src/lib/agent-catalog.ts) sort + exclude | Workspace path sync (P1) |
| [`agent-catalog-overrides.json`](../../../../apps/web/config/agent-catalog-overrides.json) | New install pipelines |
| Settings [`settings-agent-section`](../../../../apps/web/src/pages/settings/settings-agent-section.tsx) parity smoke | HostManager native session creation (pre-existing residual) |

## Acceptance criteria

| ID | Criterion | Verification |
| --- | --- | --- |
| AC-P2-1 | Native Claude/Codex show ACP-equivalent icons | Visual / override iconUrl |
| AC-P2-2 | `claude-acp` / `codex-acp` absent from default and More | Catalog unit tests + UI |
| AC-P2-3 | Installed: no Install/Docs | Installed card DOM |
| AC-P2-4 | Uninstalled curated: Install visible when whitelisted | Uninstalled Codex card |
| AC-P2-5 | Default grid order matches installed-first + curated rules | Mixed install state fixture |
| AC-P2-6 | Settings Agent section matches Setup picker behavior | Settings smoke test |
| AC-P2-7 | Forward-compat agents (Hermes/Grok/Kilo) appear when scan includes match | Optional — only when present in registry/scan |

## Architecture contract (normative — architect locked)

### Overrides schema (extend v1)

Add to `AgentOverride` / `agents.<key>`:

| Field | Rule |
| --- | --- |
| `iconUrl` | HTTPS CDN URL; pinned for natives below |
| `priority` | Integer 0–11 for curated default slots (lower = earlier among uninstalled curated) |
| `displayName` | Product card label when set |
| `excludeFromPicker` | When `true`, row omitted from **both** default grid and More |
| `description` | Optional static fallback when scan `description` is null — see § Version / description gaps (residual OK if omitted) |

Pinned icon URLs (verified 2026-07-15):

| Key | `iconUrl` |
| --- | --- |
| `claude-native` | `https://cdn.agentclientprotocol.com/registry/v1/latest/claude-acp.svg` |
| `codex-native` | `https://cdn.agentclientprotocol.com/registry/v1/latest/codex-acp.svg` |

Hard exclude: `claude-acp` and `codex-acp` → `excludeFromPicker: true` (replaces `hiddenFromDefault`-only hiding for wrappers).

### Catalog pipeline (`agent-catalog.ts`)

1. `resolveCatalogItems(entries)` — merge scan + overrides (existing).
2. **Filter** `excludeFromPicker !== true`.
3. **`defaultGridEntries`:** (a) all **installed** agents — sort `priority` asc (undefined last), then `displayName` asc; (b) **curated uninstalled** with defined `priority` 0–11 in table order; (c) remaining installed not yet listed — same sort.
4. **`moreAgentsEntries`:** scan rows not in default grid ids; sort installed-first, then name asc.

Settings Agent section imports the same exports — no fork.

### AgentCard chrome

Install and Docs render **only when** `!agent.installed` (whitelist/docs URLs unchanged).

### Wire contracts

**`wire_contracts_changed: false`**

## Open questions (architect)

~~All resolved in § Architecture contract.~~

1. ~~CDN icon URLs~~ → `/v1/latest/*.svg` pinned above.
2. ~~excludeFromPicker vs TS filter~~ → overrides boolean.
3. ~~Static descriptions~~ → optional override field; ship without if scan-only suffices.
