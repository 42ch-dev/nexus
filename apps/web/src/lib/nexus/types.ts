/**
 * `tauri-api` adapter boundary — the `NexusClient` interface.
 *
 * Spec: `.mstar/knowledge/specs/web-ui.md` §5. All daemon access from the UI
 * goes through this interface; core screen logic never calls `fetch`/`invoke`
 * directly. This is what makes the V1.65 Tauri desktop shell a one-impl swap
 * (`BrowserClient` → `TauriClient`) instead of a rewrite.
 *
 * Method coverage reflects the MVP screen groups (web-ui.md §6) against the
 * V1.64 hardened contract base (Track B / plan P0 merged): cursor pagination
 * (F-P1), the shared `ErrorResponse` (F-E1), and the findings list endpoint
 * (F-P2) are all available. Methods are typed against generated contracts so
 * no handwritten wire shapes are introduced (web-ui.md §12.6).
 *
 * Still-pending daemon surface (not in this interface; tracked as residuals):
 *  - Preset full CRUD (get/update/delete) → no daemon routes / request types
 *    exist yet; only list/scaffold/validate/reload are wired (P2 surfaces the
 *    gap; show/update/delete land in a future plan with their contracts).
 *  - Capability admission gates → CapabilityInfo carries name + I/O schemas
 *    only; admission-gate logic is not exposed in the list response.
 */
import type {
  CapabilityInfo,
  ChapterBody,
  ChapterContentQuery,
  ChapterDetail,
  ChapterOutline,
  CreateWorkRequest,
  CreateWorkResponse,
  FindingDetailResponse,
  InspectScheduleResponse,
  ListCapabilitiesResponse,
  ListChaptersQuery,
  ListChaptersResponse,
  ListFindingsQuery,
  ListFindingsResponse,
  ListPresetsResponse,
  ListSchedulesQuery,
  ListSchedulesResponse,
  ListSessionsQuery,
  ListSessionsResponse,
  ListWorksQuery,
  ListWorksResponse,
  PatchChapterRequest,
  PatchWorkRequest,
  PutChapterOutlineRequest,
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
  /** `GET /v1/local/works` — cursor list (F-P1; `works` array key, F-P3 deferred). */
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

  // ── Findings ───────────────────────────────────────────────────────────────
  /** `GET /v1/local/works/{work_id}/findings` — cursor list (F-P2; canonical `items` key). */
  listFindings(workId: string, query?: ListFindingsQuery): Promise<ListFindingsResponse>;

  // ── Preset management ─────────────────────────────────────────────────────
  /** `GET /v1/local/presets` — grouped by source. */
  listPresets(): Promise<ListPresetsResponse>;
  /** `POST /v1/local/presets` — scaffold a user preset. */
  scaffoldPreset(request: ScaffoldPresetRequest): Promise<ScaffoldPresetResponse>;
  /** `POST /v1/local/presets:validate` — dry-run validation (product-priority #1). */
  validatePreset(request: ValidatePresetRequest): Promise<ValidatePresetResponse>;
  /** `POST /v1/local/presets/{id}:reload`. */
  reloadPreset(presetId: string): Promise<ReloadPresetResponse>;

  // ── Chapters (V1.65 Content-Authoring) ─────────────────────────────────────
  /** `GET /v1/local/works/{work_id}/chapters` — cursor list (F-P3 `items` key). */
  listChapters(workId: string, query?: ListChaptersQuery): Promise<ListChaptersResponse>;
  /** `GET /v1/local/works/{work_id}/chapters/{n}` — detail + protection metadata. */
  getChapter(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterDetail>;
  /** `GET /v1/local/works/{work_id}/chapters/{n}/outline` — read outline markdown. */
  getChapterOutline(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterOutline>;
  /** `PUT /v1/local/works/{work_id}/chapters/{n}/outline` — atomic outline write. */
  putChapterOutline(
    workId: string,
    chapter: number,
    request: PutChapterOutlineRequest,
    query?: ChapterContentQuery,
  ): Promise<ChapterOutline>;
  /** `PATCH /v1/local/works/{work_id}/chapters/{n}` — structure/status update. */
  patchChapter(workId: string, chapter: number, request: PatchChapterRequest, query?: ChapterContentQuery): Promise<ChapterDetail>;
  /** `GET /v1/local/works/{work_id}/chapters/{n}/body` — read-only body markdown. */
  getChapterBody(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterBody>;
}

/** Re-exported for consumers building query/mutation hooks. */
export type { CapabilityInfo, FindingDetailResponse };
