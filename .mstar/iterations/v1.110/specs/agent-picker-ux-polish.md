# V1.110 P2 — AgentPicker UX polish (FB-D2 + FB-D4) — spec draft

> Iteration-scoped spec draft. Product-manager + architect refine in §5.1/§5.2.

## 1. Problem

`apps/web/src/components/setup/agent-picker.tsx`:

- **FB-D2** — `agents.map(...)` (:177-197) renders the full registry list
  unsorted; common agents buried; no progressive disclosure.
- **FB-D4** — `CustomLaunchField` (:398-480) puts `Input` (default size) +
  `Verify Agent` `Button` (`size="small"`) in a `flex flex-wrap gap-2` row
  (:422-449) at different heights.

## 2. Target (FB-D2)

- **Common-first** partition by locked priority (match `registry_agent_id`
  then `name`):
  **Codex CLI, Claude Code, Cursor CLI, OpenCode, Hermes, Kimi Code, Qoder,
  GitHub Copilot CLI, Pi, Kiro CLI**.
- **More** button after the common list; on click, render **rest**.
- `AgentPickerItem` gains optional `lastUpdated?: string` (ISO) for the
  view-model contract, **but the data path is NOT available on the wire in
  V1.110** (see Q2 lock) — the rest partition degrades to stable registry order
  until an upstream/wire change lands.

## 3. Target (FB-D4)

- Fuse `Input` + Verify into one height-aligned **input-group** (trailing inline
  button — locked in Q3). The Input fills width; the Verify button sits at the
  trailing edge inside one container that defines a single shared height.
- Preserve `AgentVerifyStatus` (`idle`/`loading`/`success`/`no-match`/`error`)
  and helper copy; keep test-ids stable or migrate with test updates.

## 4. Locked decisions (architect §5.2 review)

### Q1 — "More agents" copy (product-led; architect preference noted) ⏳ PRODUCT

Deferred to the product-manager pass (§5.1 owns copy). **Architect preference:**
collapsed label **"More agents"** (clear agent-domain noun, Title Case per
DESIGN.md §Voice & Content for button labels); expanded state label
**"Fewer agents"**. Accessible disclosure via `aria-expanded` on the button and
`aria-controls` pointing at the rest list region.

### Q2 — `lastUpdated` data path: NOT on the wire → registry-order fallback + residual ✅ LOCKED

**Finding (code-verified):** `lastUpdated` is **not available** on the wire in
V1.110:

- `AgentEntry` (ACP registry manifest, `crates/nexus-contracts/src/local/acp_runtime/registry_manifest.rs`)
  carries `id`, `name`, `version`, `description`, `repository`, `authors`,
  `license`, `icon`, `distribution` — **no `updated_at`/`last_modified`**. The
  registry is an **external read-only** ACP CDN document we do not control.
- `AgentScanEntry` (`schemas/daemon-api/agent-host/agent-scan-entry.schema.json`)
  carries `name`, `registry_agent_id`, `launch_command`, `installed`,
  `version`, `description`, `icon_url` — **no `lastUpdated`**, and
  `additionalProperties: false`, so adding it **is a wire change**
  (forbidden by `wire_contracts_changed: false`).
- `CacheMeta` (`fetched_at`/`registry_version`) is **cache-level**, not
  per-agent — all agents share one mtime, so "desc by last-updated" would
  degenerate to registry order anyway.

**Verdict:** keep `AgentPickerItem.lastUpdated?: string` in the view-model
(forward-compatible), but the V1.110 rest partition sorts by **stable registry
order** (the order the scan response returns). Register residual
**`R-V110P2-001`**: "FB-D2-001 'sorted by last-updated desc' requires an
upstream ACP registry `updated_at` (external dep) OR a V1.111+ scan-response
wire change — blocked under V1.110 `wire_contracts_changed: false`."

**FB-D2-001 feasibility:** the **More button + progressive disclosure IS
achievable** in V1.110; only the specific "desc by last-updated" ordering within
the rest partition degrades to registry order. The user's underlying goal
(progressively disclose the long tail) is met.

**Optional non-wire enhancement (product call):** sort the rest partition by
`version` desc as a "most-recently-updated" proxy. Semver-desc is an imperfect
proxy (prerelease tags, non-semver versions) and could mislead, so the safe
default is registry order; version-desc is a product toggle if they prefer it
over flat registry order.

### Q3 — Fused verify treatment: trailing inline button in an input-group ✅ LOCKED

**Verdict:** **trailing inline button inside an input-group container** (not a
field-affix decoration).

- The Input fills the available width; the Verify button sits at the trailing
  edge inside one container (`relative`/`flex` wrapper) that defines a single
  shared height. The button is sized to match the Input's height (drop the
  current `size="small"` mismatch — the bug is Input default + Button small).
- **Why not field-affix:** a decorative affix inside the input would require
  moving the verify action onto the input element (e.g. `onClick` on the
  affix), which is less accessible and harder to express the
  `loading`/`success`/`no-match`/`error` states. The button stays a real
  `<button>` so `disabled`, the `Loader2` spinner, and the test-ids stay intact.
- **Preserved:** `AgentVerifyStatus` state machine, `variant="secondary"`, and
  test-ids `agent-picker-verify`, `agent-picker-verify-success`,
  `agent-picker-verify-error`. No new `@42ch/nexus-ui` primitive required —
  compose with a wrapper `div` + existing `Input`/`Button`.

## 5. Constraints (Global)

- **`AgentPicker` stays presentational** (no daemon client, no wire DTOs) — the
  `lastUpdated`/ordering data flows in via `AgentPickerItem` / props.
- **No wire/schema change** (`wire_contracts_changed: false`). The
  `lastUpdated` view-model field is optional and unsourced in V1.110.
- **DESIGN.md tokens only** (no fabricated values).
- **WCAG 2.1 AA:** keyboard reach + `aria-expanded`/`aria-controls` on More;
  focus-visible ring; the fused control is one tab-stop with an accessible
  label.
- **Priority list is match-tolerant** — match by `registry_agent_id` first,
  then `name`; unknown common ids degrade gracefully to registry order within
  each partition.
- **No new `@42ch/nexus-ui` primitive** for the fused field — compose from
  existing `Input` + `Button` in a wrapper.
