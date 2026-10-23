# Agent Profile Service Persistence

| Attribute | Value |
| --- | --- |
| **Status** | Active |
| **Owner** | Daemon API (agent-host family) + web client seam |
| **Origin** | I-000064, delivered in iteration v1.220-developer-shell (plan `v1220-harness-profile`); promoted from the iteration package by its Phase 3 compound round |
| **Related** | [schemas-directory-layout.md](../contracts/schemas-directory-layout.md), [schemas-external-consumer-boundary.md](../contracts/schemas-external-consumer-boundary.md), compass decision D6 (single additive daemon-api exception of the web rebuild IA) |
| **Invariants** | Desktop IPC path unchanged; browser path persists on the service; one daemon-global profile |

## 1. Problem

The saved agent profile was reachable from only two places: the desktop
`get_agent_profile` / `set_agent_profile` IPC (Electron `agent-host/config.toml`)
and a browser-only `localStorage` snapshot keyed by creator id
(`nexus-agent-profile:<creator_id>`). A browser build therefore could not
persist the profile on the service, and the profile was not shared across
browsers/devices pointing at the same daemon. The service-owned surface closes
that gap additively: the desktop path keeps working unchanged, and the browser
path gains service persistence.

## 2. Wire surface (additive)

Both routes live in the existing agent-host family (home of
`POST /v1/daemon/agent-host/scan`); no new domain module.

### `GET /v1/daemon/agent-host/profile`

- Auth tier: `tier1` (API-key only, no active-creator requirement).
- Always `200`. Body: `AgentProfileResponse`
  - `{ "profile": { "name": string, "launch_command"?: string } | null }`
  - `null` means "no profile stored yet" — zero new error-fold rows.
- Malformed/unreadable `config.toml` is an internal error (never silently
  reported as `null`, which would let the next write drop the other keys).

### `PUT /v1/daemon/agent-host/profile`

- Auth tier: `tier1`.
- Request: `AgentProfile` — `{ "name": string, "launch_command"?: string }`.
- Runtime validation (schema is documentation, not the validator):
  non-object body, missing/non-string `name`, or non-string `launch_command`
  → `invalid_input` (400). Blank/whitespace-only `name` → `invalid_input` (400).
  Length caps: `name` ≤ 200 characters, `launch_command` ≤ 4096 characters,
  measured in code points on the trimmed value — oversized values are
  `invalid_input` (400) and never touch the stored profile.
- Success `200` with the stored profile: `{ "profile": { … } }`.
- Name and `launch_command` are stored trimmed; an empty command after trimming
  is stored as "no command" (the key is omitted).
- **Merge semantics**: omitting `launch_command` preserves the stored command
  only when the stored profile's `name` equals the incoming `name` (a name-only
  save never clears its own command); switching to a different agent with an
  omitted command stores the new name with no command. An explicit empty string
  clears it in both cases. The response always echoes the merged result.

Schemas: `schemas/daemon-api/agent-host/agent-profile.schema.json` and
`schemas/daemon-api/agent-host/agent-profile-response.schema.json`; generated
out under `packages/nexus-contracts/src/generated/daemon-api/agent-host/`.
The request body reuses the `AgentProfile` type (same shape as the nested
response object).

## 3. Storage

- Location: the daemon home `config.toml` (`<user_home>/.nexus42/config.toml` —
  the same file the core owns for `active_creator_id` and workspace path
  config; the service `home` is the raw user home, so the `.nexus42` segment is
  resolved explicitly and the directory is created before the temp write).
- Shape: one `[agent_profile]` table with `name` and optional `launch_command`.
  `name` is a TOML string; `launch_command` is omitted when unset.
- ONE daemon-global profile — the desktop semantics. It is deliberately **not**
  per-creator; the per-creator localStorage key was a browser artifact.
- Read semantics mirror the core CLI config (`read_cli_config`): a missing file
  is an empty document; an existing unparseable file is an error.
- Write semantics mirror `write_cli_config` plus crash atomicity: read the whole
  document, replace only the `agent_profile` table, every other key is
  preserved, then write a same-directory temp file, `fsync` its content, `rename`
  it into place and `fsync` the directory entry. `mode 0600` on the temp file;
  on failure the temp file is removed and the underlying cause is logged while
  the HTTP error stays generic.
- The native core may write the same file (creator/workspace selection). Both
  writers are whole-document read-modify-write; the service's writes are
  atomic at the visibility level, so a reader never sees a partial document.

### Risk acceptance (cross-writer race) — now a LIVE shared file

The TypeScript service is a second writer of `<user_home>/.nexus42/config.toml`
alongside the Rust core, and both do whole-document read-modify-write. With the
store at the daemon path this is no longer theoretical: a service `PUT` and a
core write (creator activation, workspace selection) target the same file. A
lost update is therefore possible — if the core and the service interleave read
and write within the same millisecond-scale window, one side's key can be
dropped (e.g. a creator activation landing between the profile read and the
profile write). The exposure stays bounded: the service writes only on an
explicit manual profile save (never on a timer or on read), core writes are
equally rare, no user data beyond the last write is at risk, and no in-process
lock can cover the core's write path. The acceptance therefore stands under the
live condition. **Accepted by the PM decision on 2026-10-23**, tracked as issue
I-000065 with the migration trigger (a lost update observed in practice, or any
third writer added → move to a file lock or a core-owned write). Do not add
ad-hoc locking here.

## 4. Browser migration read (legacy localStorage)

Read order in the web seam (`useAgentProfile`, `apps/web/src/api/queries.ts`):

1. Service `GET /v1/daemon/agent-host/profile`.
2. If `profile` is non-null → use it. The legacy localStorage snapshot is
   neither consulted nor written from then on.
3. If `profile` is `null` → read the legacy per-creator snapshot
   (`nexus-agent-profile:<activeCreatorId>`) once:
   - snapshot exists → best-effort `PUT` it to the service and use it;
   - PUT failure → keep using the snapshot and leave it in localStorage for the
     next load (best-effort migration, never a local-only write path);
   - no snapshot → resolve `null`.
4. New browser saves go through `PUT` only; the service remains the single
   source of truth. In the browser, the seam's retry re-reads the snapshot
   without refetching the disabled desktop IPC query; on desktop it re-runs the
   IPC read.

## 5. Web integration

- `NexusClient` exposes `getAgentProfile()` / `saveAgentProfile(profile)`;
  `BrowserClient` implements them over the routes above. Screens never fetch
  directly.
- `useAgentProfile()` is the single read seam: desktop → IPC
  (`get_agent_profile`), browser → service. `useSavedAgentProfile()` derives
  from it, so the topnav Agent chip, the developer Hub Agent card, and the
  Harness profile bar all share one source with transport-coherent saves
  (desktop saves via IPC, browser via the service).
- Onboarding (`useAgentSelection`) persists through the desktop IPC on desktop
  and through `saveAgentProfile` in the browser; the localStorage write is gone.

## 6. Desktop non-regression

- `get_agent_profile` / `set_agent_profile` IPC, their `agent-host/config.toml`
  `providers` array, and their tests are untouched.
- Desktop reads/writes never hit the service routes; the browser never depends
  on the IPC bridge.
- Desktop and browser stores are intentionally separate files at this stage
  (`agent-host/config.toml` vs home `config.toml`); this spec governs only the
  service surface. Unifying them is a separate decision.

## 7. Acceptance evidence

- Service integration test `apps/nexus-service/tests/agent-profile-http.test.mjs`
  (GET/PUT, validation, service restart on the same home, other config keys
  preserved, name-scoped merge semantics).
- Route inventory test includes both identities at `tier1`.
- Web seam tests (`src/api/saved-agent-profile.test.tsx`,
  `src/api/agent-profile.test.ts`,
  `src/components/onboarding/step-agent*.test.tsx`,
  `src/components/developer/hub-cards.test.tsx`,
  `src/components/harness/profile-bar.test.tsx`) cover service read, legacy
  migration, failed-migration fallback, transport-coherent saves, and the
  unchanged desktop IPC retry.
