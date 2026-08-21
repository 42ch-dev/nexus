/**
 * `tauri-api` adapter boundary — the `NexusClient` interface.
 *
 * Spec: `.mstar/specs/web-ui.md` §5. All daemon access from the UI
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
 *  - Capability admission gates → CapabilityInfo carries name + I/O schemas
 *    only; admission-gate logic is not exposed in the list response.
 *
 * V1.67 G2 (R-V164-P2-G2): preset get/update/delete promoted onto this
 * interface. The daemon routes + generated TS types already existed; only the
 * TS client surface was missing. A form-based management UI is deferred to the
 * V1.68 canvas (compass §0 Q6).
 *
 * Method count: see the interface below — it grows as daemon surfaces are
 * promoted (V1.78 added 5 creator-memory methods). Earlier comments carried a
 * stale literal count that drifted each release, so the count is now sourced
 * from the interface itself rather than restated in prose.
 */
import type {
  AddScheduleRequest,
  AddScheduleResponse,
  BatchUpdateFindingsRequest,
  BatchUpdateFindingsResponse,
  CapabilityInfo,
  CertFingerprintResponse,
  ChapterBody,
  ChapterContentQuery,
  ChapterDetail,
  ChapterOutline,
  CountPendingReviewsResponse,
  CreateForkRequest,
  CreateForkResponse,
  CreateWorkRequest,
  CreateWorkResponse,
  CreateWorldRequest,
  CreateWorldResponse,
  CreatorDetail,
  DeletePendingReviewResponse,
  DeleteScheduleResponse,
  EditCoreContextRequest,
  EditCoreContextResponse,
  FindingDetailResponse,
  GetPresetResponse,
  InspectScheduleResponse,
  ListCapabilitiesQuery,
  ListCapabilitiesResponse,
  ListChaptersQuery,
  ListChaptersResponse,
  ListCreatorsQuery,
  ListCreatorsResponse,
  ListFindingsQuery,
  ListFindingsResponse,
  ListMemoryFragmentsQuery,
  ListMemoryFragmentsResponse,
  ListModulesResponse,
  ListPendingReviewsQuery,
  ListPendingReviewsResponse,
  ListPresetsResponse,
  ListSchedulesQuery,
  ListSchedulesResponse,
  ListSessionsQuery,
  ListSessionsResponse,
  ListWorksQuery,
  ListWorksResponse,
  MemoryFragmentInfo,
  ModuleDetail,
  OutlinePatchChapterRequest,
  OutlinePatchResponse,
  OutlinePatchStructureRequest,
  PatchChapterRequest,
  PatchWorkRequest,
  PendingReviewInfo,
  ReadingAnnotation,
  ReadingAnnotationCreateRequest,
  ReadingAnnotationListResponse,
  ReadingAnnotationPatchRequest,
  ReadingProgressRequest,
  ReadingProgressResponse,
  ReloadPresetResponse,
  ReviewRequest,
  ReviewResponse,
  RunAcceptRequest,
  RunAcceptResponse,
  RunDetail,
  RunListResponse,
  RunRequest,
  RunResponse,
  RunSummary,
  ScaffoldPresetRequest,
  ScaffoldPresetResponse,
  ScanRequest,
  ScanResponse,
  ScheduleSummary,
  SessionDetailResponse,
  SetActiveCreatorRequest,
  SetActiveCreatorResponse,
  SignalScheduleRequest,
  SignalScheduleResponse,
  SoulNarrativeRequest,
  SoulNarrativeResponse,
  StrategyPatchPromptTemplateRequest,
  StrategyPatchResponse,
  StrategyPatchStateRequest,
  StrategyPatchTransitionRequest,
  TimelineOverviewResponse,
  TimelinePatchEventRequest,
  ListTimelineEventsResponse,
  MomentDirectiveRequest,
  MomentDirectiveResponse,
  MomentInspectRequest,
  MomentInspectResponse,
  PackExportRequest,
  PackExportResponse,
  PackImportRequest,
  PackImportResponse,
  UpdateFindingRequest,
  UpdatePresetRequest,
  UpdatePresetResponse,
  ValidatePresetRequest,
  ValidatePresetResponse,
  ActiveCreatorResponse,
  WorkDetailResponse,
  WorkOutline,
  World,
  WorldKbCandidatesResponse,
  WorldKbGraphResponse,
  WorldKbKeyBlockStateResponse,
  WorldKbPatchEntityRequest,
  WorldKbPatchEntityResponse,
  WorldKbPatchRelationshipRequest,
  WorldKbPatchRelationshipResponse,
  WorldKbPromoteCandidateRequest,
  WorldKbPromoteCandidateResponse,
  WorldFindingsListResponse,
  WorldRuleCreateRequest,
  WorldRuleResponse,
  WorldRuleUpdateRequest,
  WorldRulesListResponse,
} from '@42ch/nexus-contracts';

import type { PresetProfileResponse } from './preset-profile';

/** Daemon health probe result (`GET /v1/daemon/runtime/health`). App-side type. */
export interface DaemonHealth {
  /** `ok` when the daemon is reachable and healthy. */
  status: string;
  /** Daemon (`nexus42`) package version. */
  version: string;
}

/**
 * Query params for `GET /v1/daemon/compute/runs` (V1.147 P1). App-side type:
 * P0 shipped no generated schema for this query object; the fields mirror the
 * daemon handler's `ListRunsQuery` (`world_id` / `module_id` / `status` /
 * `limit` / `cursor`). Promote to a generated contract when a schema lands.
 */
export interface ListRunsQuery {
  /** Restrict to runs targeting this World. */
  world_id?: string;
  /** Restrict to runs of this module. */
  module_id?: string;
  /** Restrict to one lifecycle status. */
  status?: RunSummary['status'];
  /** Page size (daemon default 20, max 100). */
  limit?: number;
  /** Opaque cursor from a previous page's `next_cursor`. */
  cursor?: string;
}

/**
 * Response for `POST /v1/daemon/compute/runs/{run_id}/discard` (V1.147 P1).
 * App-side type: the daemon returns an inline `{"run_id", "status"}` JSON
 * object and P0 shipped no generated schema for it.
 */
export interface DiscardRunResponse {
  run_id: string;
  /** Always `"discarded"` on success. */
  status: 'discarded';
}

/**
 * Query params for `DELETE /v1/daemon/compute/runs` (V1.147 P3 T2 — Clear
 * history). `world_id` is required: Clear is per-World scope (the daemon 422s
 * without it) and the caller must own the World. `status` narrows Clear to one
 * terminal state (`applied|discarded|failed`) — `running` and `succeeded`
 * (needs-review) rows are never deleted.
 */
export interface ClearRunsQuery {
  /** World whose terminal runs are cleared (must be owned). */
  world_id: string;
  /** Optional terminal-state filter; absent → all terminal runs of the World. */
  status?: Extract<RunSummary['status'], 'applied' | 'discarded' | 'failed'>;
}

/**
 * Response for `DELETE /v1/daemon/compute/runs` (V1.147 P3 T2). App-side
 * type: the daemon returns an inline `{"deleted": n}` object; the plan picked
 * schema-less inline (P1 `DiscardRunResponse` precedent for trivial shapes).
 */
export interface ClearRunsResponse {
  /** Number of terminal runs deleted (applied|discarded|failed). */
  deleted: number;
}

/**
 * Query params for `GET /v1/daemon/worlds/:world_id/timeline/events`
 * (V1.147 P2). App-side type: no generated schema for this query object; the
 * fields mirror the daemon handler's `TimelineEventsParams` (branch_id /
 * status / event_type / limit / cursor). Omitted filters take the daemon
 * defaults — status defaults to `canon`, branch_id defaults to the World's
 * current branch (root fallback).
 */
export interface ListTimelineEventsQuery {
  /** Fork branch id; omitted → the World's current branch (root fallback). */
  branch_id?: string;
  /** `canon` (default) | `provisional` | `rejected`. */
  status?: 'canon' | 'provisional' | 'rejected';
  /** Exact event_type match (e.g. `compute_result`). */
  event_type?: string;
  /** Page size (daemon default 20, max 100). */
  limit?: number;
  /** Opaque cursor from a previous page's `next_cursor`. */
  cursor?: string;
}

/**
 * `PATCH /v1/daemon/orchestration/schedules/{schedule_id}` request (V1.171 P2
 * AR-29). App-side mirror of the hand-coded Rust `EditScheduleRequest` 1:1
 * (snake_case wire — local tier, NOT generated, NOT in `schemas/`). Only
 * `label` is updateable today; absent/null keeps the current label, `""`
 * clears it to NULL (never stored as an empty string).
 */
export interface EditScheduleRequest {
  /** New label. Absent → unchanged; `""` → cleared to NULL. */
  label?: string;
}

/** One role's cron entry inside the per-Work schedule (AR-29). */
export interface WorkCronRole {
  /** 5-field cron expression (author local TZ). */
  cron: string;
  /** Per-role opt-out without removing the schedule. */
  enabled: boolean;
}

/** The three-role staggering set (AR-29). */
export interface WorkCronRoles {
  /** `brainstorm` → `novel-brainstorm` preset. */
  brainstorm: WorkCronRole;
  /** `write` → `novel-write` preset. */
  write: WorkCronRole;
  /** `review` → `novel-review-master` preset. */
  review: WorkCronRole;
}

/**
 * `GET /v1/daemon/works/{work_id}/cron` response (V1.171 P2 AR-29). App-side
 * mirror of the hand-coded Rust `WorkCronResponse` 1:1. `is_default` is the
 * honesty marker (AR-30): true when `works.schedule_json` is unset and this
 * payload is the spec-default schedule.
 */
export interface WorkCronResponse {
  /** IANA timezone string. Daemon converts to UTC for cron firing. */
  tz: string;
  /** Per-role cron entries. */
  roles: WorkCronRoles;
  /** True when the stored config is unset and this is the spec defaults. */
  is_default: boolean;
}

/**
 * `PUT /v1/daemon/works/{work_id}/cron` request (V1.171 P2 AR-29). App-side
 * mirror of the hand-coded Rust `UpdateWorkCronRequest` 1:1. The body is the
 * complete `WorkSchedule` shape; `expected_current_json` is the optional CAS
 * pre-image — the exact stored `schedule_json` blob that must match for the
 * write to apply. Empty/whitespace means "must currently be unset"; absent
 * means an unconditional write.
 */
export interface UpdateWorkCronRequest {
  /** IANA timezone string. Daemon converts to UTC for cron firing. */
  tz: string;
  /** Per-role cron entries. */
  roles: WorkCronRoles;
  /**
   * CAS pre-image: the exact stored `schedule_json` blob. Pass the value
   * reconstructed from a prior `GET` (the `{ tz, roles }` shape — `is_default`
   * is not part of the stored blob) to guard against concurrent writers.
   */
  expected_current_json?: string;
}

/**
 * Transport-agnostic client for the Nexus Daemon API.
 *
 * Two implementations ship with this scaffold:
 *  - {@link BrowserClient} (V1.64) — `fetch` against same-origin `/v1/daemon/*`.
 *  - `TauriClient` (V1.65 stub) — Tauri `invoke` behind the same interface.
 *
 * Daemon API data endpoints are keyless on loopback (V1.20 model); the browser
 * client sends no credentials.
 */
export interface NexusClient {
  // ── Daemon ───────────────────────────────────────────────────────────────
  /** `GET /v1/daemon/runtime/health` — liveness + version for the shell header. */
  health(): Promise<DaemonHealth>;
  /**
   * `GET /v1/daemon/runtime/cert-fingerprint` — TOFU certificate fingerprint.
   * Intentionally unauthenticated so clients can verify the daemon before
   * sending an API key. Empty `fingerprint` means loopback-only / no remote
   * access (V1.92 P1).
   */
  certFingerprint(): Promise<CertFingerprintResponse>;
  // ── Creators (V1.94 P1) ────────────────────────────────────────────────────
  /** `GET /v1/daemon/creators` — cursor list of local creators. */
  listCreators(query?: ListCreatorsQuery): Promise<ListCreatorsResponse>;
  /**
   * `POST /v1/daemon/creators` — create a new local creator.
   * V1.94: the generated request type is not yet available; the wire shape is
   * `{ display_name: string }` and the response is {@link CreatorDetail}.
   */
  createCreator(request: { display_name: string }): Promise<CreatorDetail>;
  /**
   * `PATCH /v1/daemon/creators/{creator_id}` — update the local creator's
   * display name. V1.117: wire shape is `{ display_name: string }`; response is
   * {@link CreatorDetail}. No new generated request type.
   */
  updateCreator(creatorId: string, request: { display_name: string }): Promise<CreatorDetail>;
  /** `GET /v1/daemon/creators/active` — daemon's active creator (config.toml). */
  getActiveCreator(): Promise<ActiveCreatorResponse>;
  /** `PUT /v1/daemon/creators/active` — switch the daemon's active creator. */
  setActiveCreator(request: SetActiveCreatorRequest): Promise<SetActiveCreatorResponse>;

  // ── Agent host (V1.94 P1) ──────────────────────────────────────────────────
  /** `POST /v1/daemon/agent-host/scan` — detect locally available ACP agents. */
  scanAgents(request?: ScanRequest): Promise<ScanResponse>;

  /** `GET /v1/daemon/works` — cursor list (F-P1/F-P3/F-F1; canonical `items` key). */
  listWorks(query?: ListWorksQuery): Promise<ListWorksResponse>;
  /** `GET /v1/daemon/works/{work_id}` — full detail. */
  getWork(workId: string): Promise<WorkDetailResponse>;
  /** `POST /v1/daemon/works`. */
  createWork(request: CreateWorkRequest): Promise<CreateWorkResponse>;
  /** `POST /v1/daemon/worlds`. */
  createWorld(request: CreateWorldRequest): Promise<CreateWorldResponse>;
  /** `PATCH /v1/daemon/works/{work_id}` — status/stage/archive (free-string status). */
  patchWork(workId: string, request: PatchWorkRequest): Promise<WorkDetailResponse>;
  /**
   * `DELETE /v1/daemon/works/{work_id}` — hard-delete a Work and cascade its
   * children (chapters, findings, pool entries, etc.). V1.129 P2
   * (R-V1126P0-T2-001). Returns 204 No Content on success.
   */
  deleteWork(workId: string): Promise<void>;

  // ── Orchestration sessions ────────────────────────────────────────────────
  /** `GET /v1/daemon/orchestration/sessions` — cursor list (F-P3/F-F1; canonical `items` key). */
  listSessions(query?: ListSessionsQuery): Promise<ListSessionsResponse>;
  /** `GET /v1/daemon/orchestration/sessions/{session_id}`. */
  getSession(sessionId: string): Promise<SessionDetailResponse>;

  // ── Schedules / cron ──────────────────────────────────────────────────────
  /** `GET /v1/daemon/orchestration/schedules` — cursor list (F-P3/F-F1; canonical `items` key). */
  listSchedules(query?: ListSchedulesQuery): Promise<ListSchedulesResponse>;
  /** `GET /v1/daemon/orchestration/schedules/{schedule_id}`. */
  inspectSchedule(scheduleId: string): Promise<InspectScheduleResponse>;
  /**
   * `POST /v1/daemon/orchestration/schedules` — create a new schedule (run a
   * Strategy). V1.70 canvas Idea→Run promotion: the generated TS type + daemon
   * route already existed (V1.67 G2 pattern); only the TS client surface was
   * missing. No schema/codegen change (`wire_contracts_changed: FALSE`).
   */
  addSchedule(request: AddScheduleRequest): Promise<AddScheduleResponse>;
  /**
   * `POST /v1/daemon/orchestration/schedules/{schedule_id}/signal` — send a
   * lifecycle signal (resume / advance / pause). V1.70 canvas Idea→Resume
   * steering promotion.
   */
  signalSchedule(
    scheduleId: string,
    request: SignalScheduleRequest,
  ): Promise<SignalScheduleResponse>;
  /**
   * `PATCH /v1/daemon/orchestration/schedules/{schedule_id}/core-context` —
   * append/merge steering context (an Idea) into a running schedule's core
   * context. V1.70 canvas Idea→Steer promotion.
   */
  editCoreContext(
    scheduleId: string,
    request: EditCoreContextRequest,
  ): Promise<EditCoreContextResponse>;
  /**
   * `PATCH /v1/daemon/orchestration/schedules/{schedule_id}` — edit schedule
   * label/metadata (V1.171 P2 AR-29). Only `label` is updateable today;
   * absent keeps the current label, `""` clears it to NULL. 404 on unknown
   * id; the response is the updated {@link ScheduleSummary}.
   */
  editSchedule(scheduleId: string, request: EditScheduleRequest): Promise<ScheduleSummary>;
  /**
   * `GET /v1/daemon/works/{work_id}/cron` — effective per-Work cron config
   * (V1.171 P2 AR-29). Returns the stored `works.schedule_json` (or the spec
   * defaults when unset) plus the `is_default` honesty marker (AR-30). 404 on
   * unknown work.
   */
  getWorkCron(workId: string): Promise<WorkCronResponse>;
  /**
   * `PUT /v1/daemon/works/{work_id}/cron` — replace the per-Work cron config
   * (V1.171 P2 AR-29). Full-body `WorkSchedule` replacement, CAS-guarded via
   * `expected_current_json` (409 conflict on pre-image mismatch with a retry
   * hint; 400 with stable codes `E_CRON_INVALID_EXPR` / `E_CRON_INVALID_TZ`
   * on invalid input; 404 on unknown work).
   */
  putWorkCron(workId: string, request: UpdateWorkCronRequest): Promise<WorkCronResponse>;
  /**
   * `DELETE /v1/daemon/orchestration/schedules/{schedule_id}` — remove a
   * schedule (V1.171 P2 — PL-15/AR-31). The daemon cancels non-terminal
   * schedules before deletion and responds `200` with
   * `{ deleted: true }`; unknown ids resolve to 404. Errors (including any
   * non-terminal refusal) surface via the shared error toast — the UI does
   * not pre-filter client-side beyond the daemon's enforcement.
   */
  deleteSchedule(scheduleId: string): Promise<DeleteScheduleResponse>;

  // ── Capabilities ──────────────────────────────────────────────────────────
  /** `GET /v1/daemon/orchestration/capabilities` — cursor list (F-P3/F-F1; canonical `items` key). */
  listCapabilities(query?: ListCapabilitiesQuery): Promise<ListCapabilitiesResponse>;

  // ── Findings ───────────────────────────────────────────────────────────────
  /** `GET /v1/daemon/works/{work_id}/findings` — cursor list (F-P2; canonical `items` key). */
  listFindings(workId: string, query?: ListFindingsQuery): Promise<ListFindingsResponse>;
  /**
   * `GET /v1/daemon/works/{work_id}/findings/{finding_id}` — full finding detail.
   * V1.77 findings-remediation promotion: the daemon route + generated TS type
   * already existed (V1.67 G2 pattern); only the TS client surface was missing.
   * No schema/codegen change (`wire_contracts_changed: FALSE`).
   */
  getFinding(workId: string, findingId: string): Promise<FindingDetailResponse>;
  /**
   * `PATCH /v1/daemon/works/{work_id}/findings/{finding_id}` — remediation patch
   * (status transition / target_executor / inline edit). Server enforces the
   * 6-state lifecycle adjacency (HTTP 422 `INVALID_TRANSITION` on illegal
   * transitions); last-writer-wins, no OCC (D1b). V1.77 promotion.
   */
  updateFinding(
    workId: string,
    findingId: string,
    patch: UpdateFindingRequest,
  ): Promise<FindingDetailResponse>;

  /**
   * `PATCH /v1/daemon/findings/batch` — bulk update status and/or
   * `target_executor` for up to 100 findings. Creator-scoped; partial success
   * model with `not_found`/`conflict` arrays in the HTTP 200 response. V1.91 P1.
   */
  batchUpdateFindings(request: BatchUpdateFindingsRequest): Promise<BatchUpdateFindingsResponse>;

  // ── Preset management ─────────────────────────────────────────────────────
  /** `GET /v1/daemon/presets` — grouped by source. */
  listPresets(): Promise<ListPresetsResponse>;
  /** `POST /v1/daemon/presets` — scaffold a user preset. */
  scaffoldPreset(request: ScaffoldPresetRequest): Promise<ScaffoldPresetResponse>;
  /** `POST /v1/daemon/presets:validate` — dry-run validation (product-priority #1). */
  validatePreset(request: ValidatePresetRequest): Promise<ValidatePresetResponse>;
  /** `POST /v1/daemon/presets/{id}:reload`. */
  reloadPreset(presetId: string): Promise<ReloadPresetResponse>;
  /** `GET /v1/daemon/presets/{id}` — fetch preset manifest YAML (V1.67 G2 promotion). */
  getPreset(presetId: string): Promise<GetPresetResponse>;
  /** `PATCH /v1/daemon/presets/{id}` — update user preset YAML after validation (V1.67 G2 promotion). */
  updatePreset(presetId: string, request: UpdatePresetRequest): Promise<UpdatePresetResponse>;
  /** `DELETE /v1/daemon/presets/{id}` — delete a user preset bundle; 204 No Content (V1.67 G2 promotion). */
  deletePreset(presetId: string): Promise<void>;

  // ── Preset profiles (V1.171 P1 — AR-27) ───────────────────────────────────
  /**
   * `GET /v1/daemon/orchestration/presets/{id}/profile` — manifest-derived
   * profile (trigger lanes, states, roles, capabilities, declared signals)
   * for any resolvable preset. The app-side type mirrors the hand-coded Rust
   * DTO 1:1 (camelCase); unknown ids resolve to 404 (PL-2).
   */
  getPresetProfile(presetId: string): Promise<PresetProfileResponse>;

  // ── Strategy canvas (V1.71 Track A) ───────────────────────────────────────
  /** `POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/patch` — patch a state. */
  strategyPatchState(
    strategyId: string,
    stateId: string,
    request: StrategyPatchStateRequest,
  ): Promise<StrategyPatchResponse>;
  /** `POST /v1/daemon/strategies/{strategy_id}/transitions/patch` — rewire a transition. */
  strategyPatchTransition(
    strategyId: string,
    request: StrategyPatchTransitionRequest,
  ): Promise<StrategyPatchResponse>;
  /** `POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/prompt/patch` — patch a prompt template. */
  strategyPatchPromptTemplate(
    strategyId: string,
    stateId: string,
    request: StrategyPatchPromptTemplateRequest,
  ): Promise<StrategyPatchResponse>;

  // ── Chapters (V1.65 Content-Authoring) ─────────────────────────────────────
  /** `GET /v1/daemon/works/{work_id}/chapters` — cursor list (F-P3 `items` key). */
  listChapters(workId: string, query?: ListChaptersQuery): Promise<ListChaptersResponse>;
  /** `GET /v1/daemon/works/{work_id}/chapters/{n}` — detail + protection metadata. */
  getChapter(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterDetail>;
  /** `GET /v1/daemon/works/{work_id}/chapters/{n}/outline` — read outline markdown. */
  getChapterOutline(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterOutline>;
  /** `PATCH /v1/daemon/works/{work_id}/chapters/{n}` — structure/status update. */
  patchChapter(workId: string, chapter: number, request: PatchChapterRequest, query?: ChapterContentQuery): Promise<ChapterDetail>;
  /** `GET /v1/daemon/works/{work_id}/chapters/{n}/body` — read-only body markdown. */
  getChapterBody(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterBody>;

  // ── Outline+Timeline canvas (V1.72 Track A) ────────────────────────────────
  /** `GET /v1/daemon/works/{work_id}/outline` — work-level outline + timeline. */
  getWorkOutline(workId: string): Promise<WorkOutline>;
  /** `POST /v1/daemon/works/{work_id}/outline/patch` — structure/volume patch. */
  patchOutlineStructure(
    workId: string,
    request: OutlinePatchStructureRequest,
  ): Promise<OutlinePatchResponse>;
  /** `POST /v1/daemon/works/{work_id}/chapters/{n}/patch` — outline chapter patch. */
  patchOutlineChapter(
    workId: string,
    chapter: number,
    request: OutlinePatchChapterRequest,
  ): Promise<OutlinePatchResponse>;
  /** `POST /v1/daemon/works/{work_id}/timeline/patch` — structured timeline patch. */
  patchTimelineEvent(
    workId: string,
    request: TimelinePatchEventRequest,
  ): Promise<OutlinePatchResponse>;

  // ── World KB canvas (V1.73 Track A) ───────────────────────────────────────
  /** `GET /v1/daemon/worlds/{world_id}/kb/graph` — entity graph projection. */
  getWorldKbGraph(
    worldId: string,
    query?: { includeSuggested?: boolean },
  ): Promise<WorldKbGraphResponse>;
  /** `GET /v1/daemon/worlds/{world_id}/kb/candidates` — pending candidates. */
  getWorldKbCandidates(
    worldId: string,
    query?: { limit?: number; cursor?: string },
  ): Promise<WorldKbCandidatesResponse>;
  /** `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` — entity-level patch (per-row OCC). */
  worldKbPatchEntity(
    worldId: string,
    request: WorldKbPatchEntityRequest,
  ): Promise<WorldKbPatchEntityResponse>;
  /** `POST /v1/daemon/worlds/{world_id}/kb/promote-candidate` — adopt/reject/merge. */
  worldKbPromoteCandidate(
    worldId: string,
    request: WorldKbPromoteCandidateRequest,
  ): Promise<WorldKbPromoteCandidateResponse>;
  /** `POST /v1/daemon/worlds/{world_id}/kb/patch-relationship` — relationship add/update/remove (V1.74). */
  worldKbPatchRelationship(
    worldId: string,
    request: WorldKbPatchRelationshipRequest,
  ): Promise<WorldKbPatchRelationshipResponse>;

  // ── World check findings (V1.165 / DR-64 surfacing) ──────────────────────
  /**
   * `GET /v1/daemon/worlds/{world_id}/findings` — world-scoped check findings
   * (mental pair + rule-derived). Newest-first, 500-cap with an honest
   * `truncated` flag. Spoke vocabulary verbatim (`info|warning|error` severity,
   * `open|resolved|dismissed` status — no nexus remap). Read-only surface
   * (PD-2): the panel never writes.
   */
  listWorldFindings(worldId: string): Promise<WorldFindingsListResponse>;

  // ── World rules (V1.166 P1 route / DR-64 surfacing half; V1.169 P1 authoring) ─
  /**
   * `GET /v1/daemon/worlds/{world_id}/rules` — world-scoped structured rules
   * (V1.166 AR-3). Author-metadata list in `canonical_name ASC, rule_id ASC`
   * order, 500-cap with an honest `truncated` flag; each item projects the
   * spoke Rule vocabulary verbatim plus the AR-2 constraint carrier
   * first-class.
   */
  listWorldRules(worldId: string): Promise<WorldRulesListResponse>;
  /**
   * `POST /v1/daemon/worlds/{world_id}/rules` — create a structured rule
   * (V1.169 P1, AR-5). The request is the AR-2 constraint carrier plus
   * author metadata; the daemon mints `rule_id` server-side and applies the
   * AR-3 create defaults (`status` → `active`, `kind` → `rule`,
   * `severity_hint` → NULL → evaluation `warning`, `target_entry_types` →
   * `[]`). 400 `invalid_input` carries the AR-2 field-level `details`
   * (`{ field, reason }`) that the authoring form echoes onto the matching
   * field.
   */
  createWorldRule(worldId: string, body: WorldRuleCreateRequest): Promise<WorldRuleResponse>;
  /**
   * `PATCH /v1/daemon/worlds/{world_id}/rules/{rule_id}` — per-field edit
   * (V1.169 P1, AR-3/AR-5): `constraint` is whole-carrier replacement,
   * absent fields stay unchanged, no null-clearing. `status: "deprecated"`
   * is the Deactivate recovery (product lock — no DELETE route).
   */
  updateWorldRule(
    worldId: string,
    ruleId: string,
    body: WorldRuleUpdateRequest,
  ): Promise<WorldRuleResponse>;

  // ── Compute modules (V1.114 P2) ─────────────────────────────────────────
  /** `GET /v1/daemon/compute/modules` — cursor list of registered compute modules. */
  getComputeModules(): Promise<ListModulesResponse>;
  /** `GET /v1/daemon/compute/modules/{module_id}` — full module manifest. */
  getComputeModule(moduleId: string): Promise<ModuleDetail>;
  /**
   * `GET /v1/daemon/worlds/{world_id}/kb/key-blocks/{key_block_id}/state` —
   * mutable runtime state of a computable KeyBlock plus computability flag + OCC version.
   */
  getKeyBlockState(worldId: string, keyBlockId: string): Promise<WorldKbKeyBlockStateResponse>;

  // ── Compute runs (V1.147 P1) ──────────────────────────────────────────────
  /**
   * `POST /v1/daemon/compute/run` — invoke a module against an owned World.
   * The World is not mutated; `status: "succeeded"` carries proposals for
   * review-then-accept, `status: "failed"` carries an honest error.
   */
  runCompute(request: RunRequest): Promise<RunResponse>;
  /**
   * `POST /v1/daemon/compute/runs/{run_id}/accept` — atomically commit a
   * succeeded Run's proposals. `request` omitted/`null` accepts everything;
   * the client still sends a `{}` JSON body (axum `Json` extractor).
   */
  acceptRun(runId: string, request?: RunAcceptRequest | null): Promise<RunAcceptResponse>;
  /** `POST /v1/daemon/compute/runs/{run_id}/discard` — drop a succeeded Run's proposals. */
  discardRun(runId: string): Promise<DiscardRunResponse>;
  /** `GET /v1/daemon/compute/runs` — cursor-paginated run history (newest-first). */
  listRuns(query?: ListRunsQuery): Promise<RunListResponse>;
  /** `GET /v1/daemon/compute/runs/{run_id}` — full detail incl. proposals/error. */
  getRun(runId: string): Promise<RunDetail>;
  /**
   * `DELETE /v1/daemon/compute/runs` — Clear history (V1.147 P3 T2): deletes
   * terminal runs (`applied|discarded|failed`) for an owned World. Running and
   * succeeded (needs-review) rows are never deleted.
   */
  clearRuns(query: ClearRunsQuery): Promise<ClearRunsResponse>;

  // ── World timeline events (V1.147 P2) ───────────────────────────────────
  /**
   * `GET /v1/daemon/worlds/:world_id/timeline/events` — cursor-paginated
   * per-World timeline log events (production `narrative_timeline_events`
   * storage; machine-written families like `compute_result` carry compute
   * provenance in `extensions`). Omitted filters take the daemon defaults:
   * status `canon`, branch = the World's current branch (root fallback).
   */
  getTimelineEvents(
    worldId: string,
    query?: ListTimelineEventsQuery,
  ): Promise<ListTimelineEventsResponse>;
  /**
   * `POST /v1/daemon/worlds/:world_id/forks` — create a local timeline fork
   * (V1.162 P1 T2 route, thin delegate to the `fork.create` capability): a
   * new `branch_id` within the owned World diverging from the picked
   * fork-point event on the stated parent branch. The response carries the
   * new `branch_id` (+ parent + fork-point + `created_at`) which the World
   * Timeline consumes immediately for the PD-6 post-create landing.
   * 422 (`invalid_input`) = bad / non-existent fork point; 403 = foreign
   * World (should not occur from an owned World Timeline).
   */
  createFork(worldId: string, request: CreateForkRequest): Promise<CreateForkResponse>;

  // ── Creator Memory review-loop (V1.78) ─────────────────────────────────────
  // All memory endpoints are creator-scoped: the daemon rejects a `creator_id`
  // that does not match the active creator in config.toml with 403. The UI is
  // review/consume-only — `createPendingReview` stays CLI/producer-only (the
  // session-end capture pipeline owns `POST .../memory/pending-review`), mirroring
  // V1.77's `createFinding` CLI-only decision (compass D-UX LOCKED).
  /**
   * `GET /v1/daemon/narrative/worlds` — workspace-scoped world list for the
   * active creator. Returns every Work-backed world (including zero-fragment
   * worlds) so the SOUL world selector can surface honest subset-empty states.
   * V1.82: typed against the generated `World` domain contract; the response
   * shape is promoted to a generated list response once P0 lands the schema.
   */
  listNarrativeWorlds(): Promise<World[]>;
  /**
   * `DELETE /v1/daemon/worlds/{world_id}` — hard-delete a World and cascade its
   * KB + timelines (FK `ON DELETE CASCADE`). Works that referenced the World
   * are preserved with `world_id = NULL` (architect lock — V1.129 P2).
   * Returns 204 No Content on success.
   */
  deleteWorld(worldId: string): Promise<void>;
  /**
   * `GET /v1/daemon/timeline/overview` — cursor-paginated overview of visible
   * Worlds with per-World era/event counts and last activity timestamp.
   * V1.126 P2: replaces the N=5–10 parallel `kb/graph` fan-out in the global
   * Timeline view with one composite call.
   */
  getTimelineOverview(cursor?: string): Promise<TimelineOverviewResponse>;
  /**
   * `GET /v1/daemon/memory/pending-review?creator_id={id}` — cursor-paginated
   * pending-review list for the active creator (default 50, max 250; cursor is
   * the opaque `next_cursor` from a previous page).
   */
  listPendingReviews(
    creatorId: string,
    query?: Omit<ListPendingReviewsQuery, 'creator_id'>,
  ): Promise<ListPendingReviewsResponse>;
  /**
   * `GET /v1/daemon/memory/pending-review/count?creator_id={id}` — live pending
   * count for the active creator, surfaced as a badge on the Memory page header.
   */
  countPendingReviews(creatorId: string): Promise<CountPendingReviewsResponse>;
  /**
   * `DELETE /v1/daemon/memory/pending-review/{pending_id}?creator_id={id}` —
   * delete one pending-review row; echoes the path `pending_id`. A missing or
   * non-owned row surfaces as an error envelope, not `success: false`.
   */
  deletePendingReview(
    pendingId: string,
    creatorId: string,
  ): Promise<DeletePendingReviewResponse>;
  /**
   * `POST /v1/daemon/memory/review` — trigger the review/summarization pipeline
   * for the active creator's entire pending queue. Shipped behavior: the
   * passthrough classifier promotes/fragments/drops each pending row and returns
   * `{ promoted, fragmented, dropped }` (no LLM-backed summarizer).
   */
  reviewMemory(request: ReviewRequest): Promise<ReviewResponse>;
  /**
   * `GET /v1/daemon/memory/fragments?creator_id={id}` — read-only long-term
   * memory fragments for the active creator (NOT paginated; optional
   * case-insensitive `keyword` LIKE filter, default 50, max 250). Fragments are
   * produced only by the `reviewMemory` route — no manual CRUD on this surface.
   */
  listMemoryFragments(
    creatorId: string,
    query?: Omit<ListMemoryFragmentsQuery, 'creator_id'>,
  ): Promise<ListMemoryFragmentsResponse>;
  /**
   * `POST /v1/daemon/memory/soul/reflect` — read or regenerate the cached
   * whole-Creator SOUL narrative (V1.81). With `force_regenerate` absent/false
   * the endpoint returns the current cache state (`ungenerated` / `current` /
   * `stale` / `insufficient_data`); with `force_regenerate: true` it
   * regenerates the narrative and returns the fresh synthesis. Per-world
   * narratives are out of scope (the narrative is world-agnostic by contract).
   */
  reflectSoulNarrative(request: SoulNarrativeRequest): Promise<SoulNarrativeResponse>;

  // ── Reading depth (V1.89) ──────────────────────────────────────────────────
  /**
   * `GET /v1/daemon/reading/progress?work_id=&chapter=` — persisted scroll
   * progress for the current creator on a chapter. Returns `scroll_progress=0`
   * with a server-generated `updated_at` when no progress has been saved.
   */
  getReadingProgress(workId: string, chapter: number): Promise<ReadingProgressResponse>;
  /** `PUT /v1/daemon/reading/progress` — upsert scroll progress. */
  putReadingProgress(request: ReadingProgressRequest): Promise<ReadingProgressResponse>;
  /** `DELETE /v1/daemon/reading/progress?work_id=&chapter=` — delete progress. */
  deleteReadingProgress(workId: string, chapter: number): Promise<void>;
  /**
   * `GET /v1/daemon/reading/annotations?work_id=&chapter=` — list annotations for
   * a chapter. No pagination; per-chapter counts are expected to stay small.
   */
  listReadingAnnotations(workId: string, chapter: number): Promise<ReadingAnnotationListResponse>;
  /** `POST /v1/daemon/reading/annotations` — create a highlight annotation. */
  createReadingAnnotation(request: ReadingAnnotationCreateRequest): Promise<ReadingAnnotation>;
  /** `PATCH /v1/daemon/reading/annotations/{annotation_id}` — edit color/note. */
  patchReadingAnnotation(
    annotationId: string,
    request: ReadingAnnotationPatchRequest,
  ): Promise<ReadingAnnotation>;
  /** `DELETE /v1/daemon/reading/annotations/{annotation_id}` — delete annotation. */
  deleteReadingAnnotation(annotationId: string): Promise<void>;

  // ── Assembly Inspector (V1.151 P1 — DF-76) ────────────────────────────────
  /**
   * `POST /v1/daemon/inspector/moment` — assemble and return the enriched
   * inspector packet for one moment (activation trace + slot map + budget +
   * directive status). Read-only observation: the assembled prompt bytes are
   * never modified (AC-I6).
   */
  inspectMoment(request: MomentInspectRequest): Promise<MomentInspectResponse>;
  /**
   * `POST /v1/daemon/moment-directive` — set/show/clear the active Moment
   * Directive for an owned scope (the author write surface; DF-76). A `set`
   * without `replace` when a directive is already active in the scope
   * surfaces as a 409 conflict (no silent overwrite, CLI `--replace`
   * discipline). Returns the inserted/showed row on `set`/`show`, `{}` on
   * `clear`.
   */
  momentDirective(request: MomentDirectiveRequest): Promise<MomentDirectiveResponse>;

  // ── Narrative Knowledge Pack (V1.152 P1 — DF-77) ─────────────────────────
  /**
   * `POST /v1/daemon/worlds/:world_id/kb/pack/export` — export one World's
   * lore as a Narrative Knowledge Pack (opaque spoke envelope; entries and
   * relations are opaque objects). The response round-trips through
   * `importPack`.
   */
  exportPack(worldId: string, request?: PackExportRequest): Promise<PackExportResponse>;
  /**
   * `POST /v1/daemon/worlds/:world_id/kb/pack/import` — import a Narrative
   * Knowledge Pack into a World under a collision policy (skip / rename /
   * overwrite). Returns per-atom-type counts plus a details list.
   */
  importPack(worldId: string, request: PackImportRequest): Promise<PackImportResponse>;
}

/** Re-exported for consumers building query/mutation hooks. */
export type {
  CapabilityInfo,
  CountPendingReviewsResponse,
  CreateForkRequest,
  CreateForkResponse,
  CreatorDetail,
  DeletePendingReviewResponse,
  FindingDetailResponse,
  ListCreatorsQuery,
  ListCreatorsResponse,
  ListMemoryFragmentsQuery,
  ListPendingReviewsQuery,
  MemoryFragmentInfo,
  PendingReviewInfo,
  ReadingAnnotation,
  ReadingAnnotationCreateRequest,
  ReadingAnnotationListResponse,
  ReadingAnnotationPatchRequest,
  ReadingProgressRequest,
  ReadingProgressResponse,
  ReviewRequest,
  ReviewResponse,
  ScanRequest,
  ScanResponse,
  ActiveCreatorResponse,
  SetActiveCreatorRequest,
  SetActiveCreatorResponse,
  SoulNarrativeRequest,
  SoulNarrativeResponse,
  UpdateFindingRequest,
};
