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
  CapabilityInfo,
  ChapterBody,
  ChapterContentQuery,
  ChapterDetail,
  ChapterOutline,
  CountPendingReviewsResponse,
  CreateWorkRequest,
  CreateWorkResponse,
  DeletePendingReviewResponse,
  EditCoreContextRequest,
  EditCoreContextResponse,
  FindingDetailResponse,
  GetPresetResponse,
  InspectScheduleResponse,
  ListCapabilitiesQuery,
  ListCapabilitiesResponse,
  ListChaptersQuery,
  ListChaptersResponse,
  ListFindingsQuery,
  ListFindingsResponse,
  ListMemoryFragmentsQuery,
  ListMemoryFragmentsResponse,
  ListPendingReviewsQuery,
  ListPendingReviewsResponse,
  ListPresetsResponse,
  MemoryFragmentInfo,
  ListSchedulesQuery,
  ListSchedulesResponse,
  ListSessionsQuery,
  ListSessionsResponse,
  ListWorksQuery,
  ListWorksResponse,
  OutlinePatchChapterRequest,
  OutlinePatchResponse,
  OutlinePatchStructureRequest,
  PatchChapterRequest,
  PatchWorkRequest,
  PendingReviewInfo,
  ReloadPresetResponse,
  ReviewRequest,
  ReviewResponse,
  ScaffoldPresetRequest,
  ScaffoldPresetResponse,
  SessionDetailResponse,
  SignalScheduleRequest,
  SignalScheduleResponse,
  StrategyPatchPromptTemplateRequest,
  StrategyPatchResponse,
  StrategyPatchStateRequest,
  StrategyPatchTransitionRequest,
  WorldKbCandidatesResponse,
  WorldKbGraphResponse,
  WorldKbPatchEntityRequest,
  WorldKbPatchEntityResponse,
  WorldKbPatchRelationshipRequest,
  WorldKbPatchRelationshipResponse,
  WorldKbPromoteCandidateRequest,
  WorldKbPromoteCandidateResponse,
  TimelinePatchEventRequest,
  UpdateFindingRequest,
  UpdatePresetRequest,
  UpdatePresetResponse,
  ValidatePresetRequest,
  ValidatePresetResponse,
  WorkDetailResponse,
  WorkOutline,
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
  /** `GET /v1/local/works` — cursor list (F-P1/F-P3/F-F1; canonical `items` key). */
  listWorks(query?: ListWorksQuery): Promise<ListWorksResponse>;
  /** `GET /v1/local/works/{work_id}` — full detail. */
  getWork(workId: string): Promise<WorkDetailResponse>;
  /** `POST /v1/local/works`. */
  createWork(request: CreateWorkRequest): Promise<CreateWorkResponse>;
  /** `PATCH /v1/local/works/{work_id}` — status/stage/archive (free-string status). */
  patchWork(workId: string, request: PatchWorkRequest): Promise<WorkDetailResponse>;

  // ── Orchestration sessions ────────────────────────────────────────────────
  /** `GET /v1/local/orchestration/sessions` — cursor list (F-P3/F-F1; canonical `items` key). */
  listSessions(query?: ListSessionsQuery): Promise<ListSessionsResponse>;
  /** `GET /v1/local/orchestration/sessions/{session_id}`. */
  getSession(sessionId: string): Promise<SessionDetailResponse>;

  // ── Schedules / cron ──────────────────────────────────────────────────────
  /** `GET /v1/local/orchestration/schedules` — cursor list (F-P3/F-F1; canonical `items` key). */
  listSchedules(query?: ListSchedulesQuery): Promise<ListSchedulesResponse>;
  /** `GET /v1/local/orchestration/schedules/{schedule_id}`. */
  inspectSchedule(scheduleId: string): Promise<InspectScheduleResponse>;
  /**
   * `POST /v1/local/orchestration/schedules` — create a new schedule (run a
   * Strategy). V1.70 canvas Idea→Run promotion: the generated TS type + daemon
   * route already existed (V1.67 G2 pattern); only the TS client surface was
   * missing. No schema/codegen change (`wire_contracts_changed: FALSE`).
   */
  addSchedule(request: AddScheduleRequest): Promise<AddScheduleResponse>;
  /**
   * `POST /v1/local/orchestration/schedules/{schedule_id}/signal` — send a
   * lifecycle signal (resume / advance / pause). V1.70 canvas Idea→Resume
   * steering promotion.
   */
  signalSchedule(
    scheduleId: string,
    request: SignalScheduleRequest,
  ): Promise<SignalScheduleResponse>;
  /**
   * `PATCH /v1/local/orchestration/schedules/{schedule_id}/core-context` —
   * append/merge steering context (an Idea) into a running schedule's core
   * context. V1.70 canvas Idea→Steer promotion.
   */
  editCoreContext(
    scheduleId: string,
    request: EditCoreContextRequest,
  ): Promise<EditCoreContextResponse>;

  // ── Capabilities ──────────────────────────────────────────────────────────
  /** `GET /v1/local/orchestration/capabilities` — cursor list (F-P3/F-F1; canonical `items` key). */
  listCapabilities(query?: ListCapabilitiesQuery): Promise<ListCapabilitiesResponse>;

  // ── Findings ───────────────────────────────────────────────────────────────
  /** `GET /v1/local/works/{work_id}/findings` — cursor list (F-P2; canonical `items` key). */
  listFindings(workId: string, query?: ListFindingsQuery): Promise<ListFindingsResponse>;
  /**
   * `GET /v1/local/works/{work_id}/findings/{finding_id}` — full finding detail.
   * V1.77 findings-remediation promotion: the daemon route + generated TS type
   * already existed (V1.67 G2 pattern); only the TS client surface was missing.
   * No schema/codegen change (`wire_contracts_changed: FALSE`).
   */
  getFinding(workId: string, findingId: string): Promise<FindingDetailResponse>;
  /**
   * `PATCH /v1/local/works/{work_id}/findings/{finding_id}` — remediation patch
   * (status transition / target_executor / inline edit). Server enforces the
   * 6-state lifecycle adjacency (HTTP 422 `INVALID_TRANSITION` on illegal
   * transitions); last-writer-wins, no OCC (D1b). V1.77 promotion.
   */
  updateFinding(
    workId: string,
    findingId: string,
    patch: UpdateFindingRequest,
  ): Promise<FindingDetailResponse>;

  // ── Preset management ─────────────────────────────────────────────────────
  /** `GET /v1/local/presets` — grouped by source. */
  listPresets(): Promise<ListPresetsResponse>;
  /** `POST /v1/local/presets` — scaffold a user preset. */
  scaffoldPreset(request: ScaffoldPresetRequest): Promise<ScaffoldPresetResponse>;
  /** `POST /v1/local/presets:validate` — dry-run validation (product-priority #1). */
  validatePreset(request: ValidatePresetRequest): Promise<ValidatePresetResponse>;
  /** `POST /v1/local/presets/{id}:reload`. */
  reloadPreset(presetId: string): Promise<ReloadPresetResponse>;
  /** `GET /v1/local/presets/{id}` — fetch preset manifest YAML (V1.67 G2 promotion). */
  getPreset(presetId: string): Promise<GetPresetResponse>;
  /** `PATCH /v1/local/presets/{id}` — update user preset YAML after validation (V1.67 G2 promotion). */
  updatePreset(presetId: string, request: UpdatePresetRequest): Promise<UpdatePresetResponse>;
  /** `DELETE /v1/local/presets/{id}` — delete a user preset bundle; 204 No Content (V1.67 G2 promotion). */
  deletePreset(presetId: string): Promise<void>;

  // ── Strategy canvas (V1.71 Track A) ───────────────────────────────────────
  /** `POST /v1/local/strategies/{strategy_id}/states/{state_id}/patch` — patch a state. */
  strategyPatchState(
    strategyId: string,
    stateId: string,
    request: StrategyPatchStateRequest,
  ): Promise<StrategyPatchResponse>;
  /** `POST /v1/local/strategies/{strategy_id}/transitions/patch` — rewire a transition. */
  strategyPatchTransition(
    strategyId: string,
    request: StrategyPatchTransitionRequest,
  ): Promise<StrategyPatchResponse>;
  /** `POST /v1/local/strategies/{strategy_id}/states/{state_id}/prompt/patch` — patch a prompt template. */
  strategyPatchPromptTemplate(
    strategyId: string,
    stateId: string,
    request: StrategyPatchPromptTemplateRequest,
  ): Promise<StrategyPatchResponse>;

  // ── Chapters (V1.65 Content-Authoring) ─────────────────────────────────────
  /** `GET /v1/local/works/{work_id}/chapters` — cursor list (F-P3 `items` key). */
  listChapters(workId: string, query?: ListChaptersQuery): Promise<ListChaptersResponse>;
  /** `GET /v1/local/works/{work_id}/chapters/{n}` — detail + protection metadata. */
  getChapter(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterDetail>;
  /** `GET /v1/local/works/{work_id}/chapters/{n}/outline` — read outline markdown. */
  getChapterOutline(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterOutline>;
  /** `PATCH /v1/local/works/{work_id}/chapters/{n}` — structure/status update. */
  patchChapter(workId: string, chapter: number, request: PatchChapterRequest, query?: ChapterContentQuery): Promise<ChapterDetail>;
  /** `GET /v1/local/works/{work_id}/chapters/{n}/body` — read-only body markdown. */
  getChapterBody(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterBody>;

  // ── Outline+Timeline canvas (V1.72 Track A) ────────────────────────────────
  /** `GET /v1/local/works/{work_id}/outline` — work-level outline + timeline. */
  getWorkOutline(workId: string): Promise<WorkOutline>;
  /** `POST /v1/local/works/{work_id}/outline/patch` — structure/volume patch. */
  patchOutlineStructure(
    workId: string,
    request: OutlinePatchStructureRequest,
  ): Promise<OutlinePatchResponse>;
  /** `POST /v1/local/works/{work_id}/chapters/{n}/patch` — outline chapter patch. */
  patchOutlineChapter(
    workId: string,
    chapter: number,
    request: OutlinePatchChapterRequest,
  ): Promise<OutlinePatchResponse>;
  /** `POST /v1/local/works/{work_id}/timeline/patch` — structured timeline patch. */
  patchTimelineEvent(
    workId: string,
    request: TimelinePatchEventRequest,
  ): Promise<OutlinePatchResponse>;

  // ── World KB canvas (V1.73 Track A) ───────────────────────────────────────
  /** `GET /v1/local/worlds/{world_id}/kb/graph` — entity graph projection. */
  getWorldKbGraph(
    worldId: string,
    query?: { includeSuggested?: boolean },
  ): Promise<WorldKbGraphResponse>;
  /** `GET /v1/local/worlds/{world_id}/kb/candidates` — pending candidates. */
  getWorldKbCandidates(
    worldId: string,
    query?: { limit?: number; cursor?: string },
  ): Promise<WorldKbCandidatesResponse>;
  /** `POST /v1/local/worlds/{world_id}/kb/patch-entity` — entity-level patch (per-row OCC). */
  worldKbPatchEntity(
    worldId: string,
    request: WorldKbPatchEntityRequest,
  ): Promise<WorldKbPatchEntityResponse>;
  /** `POST /v1/local/worlds/{world_id}/kb/promote-candidate` — adopt/reject/merge. */
  worldKbPromoteCandidate(
    worldId: string,
    request: WorldKbPromoteCandidateRequest,
  ): Promise<WorldKbPromoteCandidateResponse>;
  /** `POST /v1/local/worlds/{world_id}/kb/patch-relationship` — relationship add/update/remove (V1.74). */
  worldKbPatchRelationship(
    worldId: string,
    request: WorldKbPatchRelationshipRequest,
  ): Promise<WorldKbPatchRelationshipResponse>;

  // ── Creator Memory review-loop (V1.78) ─────────────────────────────────────
  // All memory endpoints are creator-scoped: the daemon rejects a `creator_id`
  // that does not match the active creator in config.toml with 403. The UI is
  // review/consume-only — `createPendingReview` stays CLI/producer-only (the
  // session-end capture pipeline owns `POST .../memory/pending-review`), mirroring
  // V1.77's `createFinding` CLI-only decision (compass D-UX LOCKED).
  /**
   * `GET /v1/local/memory/pending-review?creator_id={id}` — cursor-paginated
   * pending-review list for the active creator (default 50, max 250; cursor is
   * the opaque `next_cursor` from a previous page).
   */
  listPendingReviews(
    creatorId: string,
    query?: Omit<ListPendingReviewsQuery, 'creator_id'>,
  ): Promise<ListPendingReviewsResponse>;
  /**
   * `GET /v1/local/memory/pending-review/count?creator_id={id}` — live pending
   * count for the active creator, surfaced as a badge on the Memory page header.
   */
  countPendingReviews(creatorId: string): Promise<CountPendingReviewsResponse>;
  /**
   * `DELETE /v1/local/memory/pending-review/{pending_id}?creator_id={id}` —
   * delete one pending-review row; echoes the path `pending_id`. A missing or
   * non-owned row surfaces as an error envelope, not `success: false`.
   */
  deletePendingReview(
    pendingId: string,
    creatorId: string,
  ): Promise<DeletePendingReviewResponse>;
  /**
   * `POST /v1/local/memory/review` — trigger the review/summarization pipeline
   * for the active creator's entire pending queue. Shipped behavior: the
   * passthrough classifier promotes/fragments/drops each pending row and returns
   * `{ promoted, fragmented, dropped }` (no LLM-backed summarizer).
   */
  reviewMemory(request: ReviewRequest): Promise<ReviewResponse>;
  /**
   * `GET /v1/local/memory/fragments?creator_id={id}` — read-only long-term
   * memory fragments for the active creator (NOT paginated; optional
   * case-insensitive `keyword` LIKE filter, default 50, max 250). Fragments are
   * produced only by the `reviewMemory` route — no manual CRUD on this surface.
   */
  listMemoryFragments(
    creatorId: string,
    query?: Omit<ListMemoryFragmentsQuery, 'creator_id'>,
  ): Promise<ListMemoryFragmentsResponse>;
}

/** Re-exported for consumers building query/mutation hooks. */
export type {
  CapabilityInfo,
  CountPendingReviewsResponse,
  DeletePendingReviewResponse,
  FindingDetailResponse,
  ListMemoryFragmentsQuery,
  ListPendingReviewsQuery,
  MemoryFragmentInfo,
  PendingReviewInfo,
  ReviewRequest,
  ReviewResponse,
  UpdateFindingRequest,
};
