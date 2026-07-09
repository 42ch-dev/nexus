# Agent Picker & Detection — V1.101 Iteration Contract

**Status:** Architect-locked (iteration-start §5.2)  
**Tier:** **Must (P0)** — required for iteration Must completeness  
**Plan:** `2026-07-09-v1.101-agent-detection-picker`  
**Closes:** `R-V1100P0SMOKE-AGENT-SCAN`  
**Deferred (out of ship scope):** Settings shell = **DF-70** (AgentPicker must remain Settings-reusable)  
**Wire:** `wire_contracts_changed: false` — any `schemas/` proposal is a **hard stop** → PM/architect (do not implement).  
**Durable scan contract:** [`.mstar/specs/desktop-shell.md`](../../../specs/desktop-shell.md) §14 (`POST /v1/daemon/agent-host/scan`) — this file owns V1.101 UI/placement/PATH-enrichment locks; do not fork the wire shapes here.

## 1. Problem

After V1.100 clean-state bootstrap, authors can reach the setup agent step, but locally installed ACP agent CLIs are not surfaced (or not selectable) in a credible product surface. The current step uses a simple list; product direction is an Open Design–inspired **card grid** with status dots and **outbound** install/docs links (local-first ACP client — use the author’s agents; no BYOK).

## 2. Goals

1. **Detect:** Use existing `POST /v1/daemon/agent-host/scan` (or a minimal documented env/PATH fix if the scan→UI path is broken) so installed agents appear.
2. **Select:** Author can select an **installed** agent and persist the agent profile (existing wizard persistence path), or continue via **custom launch**.
3. **Honest states (acceptance-visible):** Loading, empty (+ custom launch), error (+ custom launch), and selected — each covered in Studio fixtures, App wiring, and Vitest.
4. **AgentPicker (Settings-reusable without shipping Settings):** Reusable presentational/composition API with **no wizard-route coupling**, suitable for a future Settings host (DF-70). Wizard is the first consumer; Settings shell / routes are **not** shipped this iteration.
5. **Reference UX:** Card grid; status indicator (installed / not installed / selected); install + docs as **outbound** links only.

## 3. Non-Goals

- BYOK / API keys.
- In-app installers or package managers.
- Settings shell / settings routes / settings sidebar IA (**DF-70**).
- Expanding agent registry product beyond what scan already returns, unless RCA proves a minimal daemon/env fix is required.
- Wire schema changes (`wire_contracts_changed: false`; schema proposals block implement).
- Promoting `AgentPicker` into `@42ch/nexus-ui` this iteration (see §5).
- Treating interactive desktop smoke as an automated Done blocker.

## 4. Studio-first

1. Build Design Studio fixtures for AgentPicker states: loading, installed grid, mixed installed/not-installed, empty, error, selected.
2. Visual acceptance in Studio (automated Studio checks + human visual pass as needed).
3. Only then wire App: scan query, selection, profile persistence, outbound links, custom launch.

## 5. Component boundary (LOCKED)

| Layer | Owns |
|-------|------|
| `AgentPicker` (app-shared presentational/composition) | Layout, cards, status dots, selected state, install/docs link slots, empty/error slots — **no** wizard routing, setup step IDs, Settings shell, or daemon client imports |
| App / wizard (`setup-step-agent`) | Scan data fetch (`useScanAgents` / `NexusClient`), profile persistence, custom launch command, product copy, outbound URL table, desktop capabilities |
| Future Settings (**DF-70**, not this iteration) | Host shell; mounts the **same** app-shared `AgentPicker` without importing wizard pages |

### Placement (LOCKED — default for V1.101)

- **Path:** `apps/web/src/components/setup/agent-picker.tsx` (export presentational `AgentPicker` + props types from that module).
- **Not** `@42ch/nexus-ui` this iteration. Rationale: V1.99 promotion boundary keeps product compositions (setup rows / domain card grids) out of the package; Settings reuse is same-app (`apps/web`); P2 Stretch owns the only package promotion (`Select`).
- **Studio access:** Design Studio may import the presentational module via a gallery-only alias (e.g. `@web-setup/*` → `apps/web/src/components/setup/*`), mirroring `@web-ui/*`. Fixtures stay props-driven — no `@42ch/nexus-contracts` or daemon client in Studio.
- **Props contract:** App maps `AgentScanEntry` (+ static install/docs URLs) into picker view-model props. Picker must not import wire DTOs if that would force Studio to pull contracts; prefer a small local props type owned by the picker module.
- **Later promotion:** Only if a second product surface outside `apps/web` needs the same picker **and** it stays pure presentational — record as a future promotion candidate; not V1.101 Must.

## 6. Scan→UI fix boundary (LOCKED)

### RCA classes (Task 1 must classify before coding the fix)

| Class | Symptom | Fix locus |
|-------|---------|-----------|
| **A — UI** | Scan returns installed agents; UI filters, disables, or hides them | `apps/web` only (picker + `setup-step-agent`) |
| **B — Process PATH/env** | Scan returns empty / all `installed: false` because daemon `which` sees a stripped GUI PATH | Env enrichment at daemon process start — **no** `schemas/` change |
| **C — Registry / binary mismatch** | PATH has CLIs but registry `cmd` names do not match | Prefer documenting + custom-launch escape hatch; do **not** invent schema fields. Escalate to PM if product requires registry/product expansion |

Current UI already calls `filter: 'all'` and renders both installed and not-installed entries — Class A is still possible (selection rules / empty-state bugs) but Class B is the expected desktop-smoke root cause (Tauri GUI → sidecar inherits minimal macOS app PATH; Homebrew agents under `/opt/homebrew/bin` are invisible to `which::which`).

### Minimal fix for Class B (no wire change)

1. **Preferred:** Enrich `PATH` for the daemon process **before** `scan_local_installations` / `which::which` runs — shared boot helper used by CLI `nexus42 daemon start` **and** desktop sidecar spawn (or sidecar-only if RCA proves CLI PATH is already correct). Merge a login-shell-equivalent user PATH (e.g. common user bin dirs + existing process PATH) without shelling out on every scan.
2. **Allowed:** Desktop sidecar spawn sets an augmented `PATH` env on the child when spawning `nexus42` (`apps/desktop` sidecar) if that alone closes the desktop reproduction.
3. **Forbidden without PM/architect re-open:** Changing `ScanRequest` / `ScanResponse` / `AgentScanEntry`; new scan endpoints; returning PATH diagnostics on the wire; rewriting the ACP registry product model; loosening PATH-probe safety (`desktop-shell.md` §14.3).
4. **Hard stop:** Any proposed `schemas/` edit → stop implement, return to PM/architect with RCA evidence.

Existing scan contract stays: `POST /v1/daemon/agent-host/scan` + generated `@42ch/nexus-contracts` types.

## 7. Selection & card rules

| Card / path | Selectable as profile? | Notes |
|-------------|------------------------|-------|
| Installed agent | Yes | Primary happy path; persist via existing profile path |
| Known / not installed | No | Discoverability only; status dot + outbound install/docs when URL present |
| Custom launch | Yes (escape hatch) | Always available from empty and error; also reachable when list is non-empty |

## 8. Install / docs links (outbound-only)

- Configured per agent id/name via a **static URL table in the app layer** (wizard / setup module) — not in `@42ch/nexus-ui`, not via schema.
- Missing URL → **hide** that link (do not show a dead control).
- Click opens system/browser URL; **no** in-app download, installer, or package manager.
- Acceptance must show at least one fixture/path where links appear and one where they are hidden.

## 9. Acceptance (automated path) — blocks automated Done

- Studio fixtures cover required visual states (including empty, error, custom-launch affordance, outbound link slots).
- Vitest covers selection of installed agent, empty, error, custom launch, and persistence wiring.
- If Class B fix lands: unit/integration evidence that PATH enrichment makes a known binary resolve (daemon/sidecar test), without schema churn.
- Residual `R-V1100P0SMOKE-AGENT-SCAN` closed or re-scoped on **automated** evidence; PM records disposition.
- `AgentPicker` has no wizard-route imports (Settings-reusable product bar met without shipping Settings).

## 10. Human smoke (separate gate) — does **not** block automated Done

Interactive desktop confirmation that real PATH-installed CLIs appear and are selectable. Scheduled **outside** the automated drive / CI QA checklist. Automated Done ≠ human smoke Done; PM schedules smoke after automated paths land.

## 11. Architect decisions (resolved)

| Question | Decision |
|----------|----------|
| Scan→UI if PATH/env root cause | Class B → process PATH enrichment at daemon/sidecar start; **no** `schemas/` change. Schema proposals = hard stop. |
| AgentPicker placement | **App-shared** at `apps/web/src/components/setup/agent-picker.tsx`. Do **not** promote to `@42ch/nexus-ui` in V1.101. |
