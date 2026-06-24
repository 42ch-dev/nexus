/**
 * `tauri-api` adapter boundary — the `NexusClient` interface.
 *
 * Spec: `.mstar/knowledge/specs/web-ui.md` §5. All daemon access from the UI
 * goes through this interface; core screen logic never calls `fetch`/`invoke`
 * directly. This is what makes the V1.65 Tauri desktop shell a one-impl swap
 * (`BrowserClient` → `TauriClient`) instead of a rewrite.
 *
 * Method coverage reflects the MVP screen groups (web-ui.md §6) **constrained
 * to contracts types that exist at the V1.63 contract base this scaffold
 * builds against**. Methods that depend on Track-B hardening (V1.64 plan P0)
 * are intentionally omitted and tracked below; they land with their contracts
 * types so no handwritten wire shapes are introduced (web-ui.md §12.6).
 *
 * Pending contracts alignment (lands with plan P0 on the integration branch):
 *  - `listFindings` → blocked on `ListFindingsResponse` (F-P2 endpoint). The
 *    Findings screen is a placeholder route until then.
 *  - Works list cursor → `ListWorksResponse` is offset/limit `{ works, total }`
 *    at this base; P0 migrates it to cursor + `PaginationInfo`. `listWorks`
 *    takes the current `ListWorksQuery` and returns the current shape.
 *  - Preset full CRUD (get/update/delete) → no daemon routes / request types
 *    yet; only list/scaffold/validate/reload are wired.
 *  - `getWork`/`patchWork` typed against generated `WorkDetailResponse`; the
 *    daemon currently emits hand-written `WorkApiDto` (drift R-V163-P1-T6),
 *    which P0 closes.
 */
import type {
  CapabilityInfo,
  CreateWorkRequest,
  CreateWorkResponse,
  InspectScheduleResponse,
  ListCapabilitiesResponse,
  ListPresetsResponse,
  ListSchedulesQuery,
  ListSchedulesResponse,
  ListSessionsQuery,
  ListSessionsResponse,
  ListWorksQuery,
  ListWorksResponse,
  PatchWorkRequest,
  ReloadPresetResponse,
  ScaffoldPresetRequest,
  ScaffoldPresetResponse,
  SessionDetailResponse,
  ValidatePresetRequest,
  ValidatePresetResponse,
  WorkDetailResponse,
} from '@42ch/nexus-contracts';

/** Daemon health probe result (`GET /v1/local/runtime/health`). App-side type. */
export interface DaemonHealth {
  /** `ok` when the daemon is reachable and healthy. */
  status: string;
  /** Daemon (`nexus42`) package version. */
  version: string;
}

/**
 * Transport-agnostic client for the Nexus Local API.
 *
 * Two implementations ship with this scaffold:
 *  - {@link BrowserClient} (V1.64) — `fetch` against same-origin `/v1/local/*`.
 *  - `TauriClient` (V1.65 stub) — Tauri `invoke` behind the same interface.
 *
 * Local API data endpoints are keyless on loopback (V1.20 model); the browser
 * client sends no credentials.
 */
export interface NexusClient {
  // ── Daemon ───────────────────────────────────────────────────────────────
  /** `GET /v1/local/runtime/health` — liveness + version for the shell header. */
  health(): Promise<DaemonHealth>;

  // ── Works (dashboard + Work CRUD) ─────────────────────────────────────────
  /** `GET /v1/local/works` — cursor list (offset/limit until P0 F-P1). */
  listWorks(query?: ListWorksQuery): Promise<ListWorksResponse>;
  /** `GET /v1/local/works/{work_id}` — full detail. */
  getWork(workId: string): Promise<WorkDetailResponse>;
  /** `POST /v1/local/works`. */
  createWork(request: CreateWorkRequest): Promise<CreateWorkResponse>;
  /** `PATCH /v1/local/works/{work_id}` — status/stage/archive (free-string status). */
  patchWork(workId: string, request: PatchWorkRequest): Promise<WorkDetailResponse>;

  // ── Orchestration sessions ────────────────────────────────────────────────
  /** `GET /v1/local/orchestration/sessions`. */
  listSessions(query?: ListSessionsQuery): Promise<ListSessionsResponse>;
  /** `GET /v1/local/orchestration/sessions/{session_id}`. */
  getSession(sessionId: string): Promise<SessionDetailResponse>;

  // ── Schedules / cron ──────────────────────────────────────────────────────
  /** `GET /v1/local/orchestration/schedules`. */
  listSchedules(query?: ListSchedulesQuery): Promise<ListSchedulesResponse>;
  /** `GET /v1/local/orchestration/schedules/{schedule_id}`. */
  inspectSchedule(scheduleId: string): Promise<InspectScheduleResponse>;

  // ── Capabilities ──────────────────────────────────────────────────────────
  /** `GET /v1/local/orchestration/capabilities`. */
  listCapabilities(): Promise<ListCapabilitiesResponse>;

  // ── Preset management ─────────────────────────────────────────────────────
  /** `GET /v1/local/presets` — grouped by source. */
  listPresets(): Promise<ListPresetsResponse>;
  /** `POST /v1/local/presets` — scaffold a user preset. */
  scaffoldPreset(request: ScaffoldPresetRequest): Promise<ScaffoldPresetResponse>;
  /** `POST /v1/local/presets:validate` — dry-run validation (product-priority #1). */
  validatePreset(request: ValidatePresetRequest): Promise<ValidatePresetResponse>;
  /** `POST /v1/local/presets/{id}:reload`. */
  reloadPreset(presetId: string): Promise<ReloadPresetResponse>;
}

/** Re-exported for consumers building query/mutation hooks. */
export type { CapabilityInfo };
