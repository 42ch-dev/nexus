/**
 * `BrowserClient` — the V1.64 NexusClient implementation for the browser.
 *
 * Spec: web-ui.md §5. Uses `fetch` against same-origin `/v1/daemon/*`. In dev
 * the Vite dev server proxies these requests to the running daemon
 * (vite.config.ts, default http://127.0.0.1:8420); in release the daemon serves
 * the embedded SPA at `/` and the Daemon API at `/v1/daemon/*` on the same port.
 *
 * Daemon API data endpoints are keyless on loopback (V1.20 model),
 * so this client sends no credentials.
 */
import type {
  AddScheduleRequest,
  AddScheduleResponse,
  BatchUpdateFindingsRequest,
  BatchUpdateFindingsResponse,
  CertFingerprintResponse,
  ChapterBody,
  ChapterContentQuery,
  ChapterDetail,
  ChapterOutline,
  CountPendingReviewsResponse,
  CreateWorkRequest,
  CreateWorkResponse,
  CreatorDetail,
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
  ModuleDetail,
  OutlinePatchChapterRequest,
  OutlinePatchResponse,
  OutlinePatchStructureRequest,
  PatchChapterRequest,
  PatchWorkRequest,
  ReadingAnnotation,
  ReadingAnnotationCreateRequest,
  ReadingAnnotationListResponse,
  ReadingAnnotationPatchRequest,
  ReadingProgressRequest,
  ReadingProgressResponse,
  ReloadPresetResponse,
  ReviewRequest,
  ReviewResponse,
  ScaffoldPresetRequest,
  ScaffoldPresetResponse,
  ScanRequest,
  ScanResponse,
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
  UpdateFindingRequest,
  UpdatePresetRequest,
  UpdatePresetResponse,
  ValidatePresetRequest,
  ValidatePresetResponse,
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
} from '@42ch/nexus-contracts';

import { NexusClientError, type TransportErrorKind } from './errors';
import type { DaemonHealth, NexusClient } from './types';

export interface BrowserClientOptions {
  /**
   * Origin/base path prefix for Daemon API requests. Defaults to `''` (same
   * origin, relative). Set only if the API is served from a different origin
   * than the SPA shell.
   */
  baseUrl?: string;
  /**
   * Optional API key for remote daemon access. When set, every protected
   * request carries an `X-API-Key` header. The fingerprint endpoint is the
   * only daemon route that intentionally requires no key.
   */
  apiKey?: string;
  /** Optional fetch implementation (testing/diagnostics injection). */
  fetchImpl?: typeof fetch;
}

type QueryValue = string | number | boolean | undefined | null;

/**
 * Serialize a query object into a `?a=b&c=d` string, omitting empty values.
 * Accepts a plain `object` so generated query DTOs (interfaces, no index
 * signature) pass without casts.
 */
function toQueryString(query: object | undefined): string {
  if (!query) return '';
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value === undefined || value === null || value === '') continue;
    params.append(key, String(value as QueryValue));
  }
  const qs = params.toString();
  return qs ? `?${qs}` : '';
}

export class BrowserClient implements NexusClient {
  private readonly baseUrl: string;
  private readonly apiKey: string | undefined;
  private readonly fetchImpl: typeof fetch;

  constructor(options: BrowserClientOptions = {}) {
    this.baseUrl = (options.baseUrl ?? '').replace(/\/+$/, '');
    this.apiKey = options.apiKey;
    this.fetchImpl = options.fetchImpl ?? fetch.bind(globalThis);
  }

  // ── Daemon ─────────────────────────────────────────────────────────────────
  health(): Promise<DaemonHealth> {
    return this.get<DaemonHealth>('/v1/daemon/runtime/health');
  }

  certFingerprint(): Promise<CertFingerprintResponse> {
    // The fingerprint endpoint is intentionally unauthenticated and publicly
    // reachable so clients can perform TOFU verification before sending the
    // API key. It is the only daemon route that deliberately omits the key.
    return this.request<CertFingerprintResponse>('GET', '/v1/daemon/runtime/cert-fingerprint', undefined, false);
  }

  // ── Creators + Agent host (V1.94 P1) ───────────────────────────────────────
  listCreators(query?: ListCreatorsQuery): Promise<ListCreatorsResponse> {
    return this.get<ListCreatorsResponse>('/v1/daemon/creators', query);
  }
  createCreator(request: { display_name: string }): Promise<CreatorDetail> {
    return this.post<CreatorDetail>('/v1/daemon/creators', request);
  }
  updateCreator(creatorId: string, request: { display_name: string }): Promise<CreatorDetail> {
    return this.patch<CreatorDetail>(
      `/v1/daemon/creators/${encodeURIComponent(creatorId)}`,
      request,
    );
  }
  setActiveCreator(request: SetActiveCreatorRequest): Promise<SetActiveCreatorResponse> {
    return this.post<SetActiveCreatorResponse>('/v1/daemon/creators/active', request);
  }
  scanAgents(request?: ScanRequest): Promise<ScanResponse> {
    return this.post<ScanResponse>('/v1/daemon/agent-host/scan', request);
  }

  // ── Works ──────────────────────────────────────────────────────────────────
  listWorks(query?: ListWorksQuery): Promise<ListWorksResponse> {
    return this.get<ListWorksResponse>('/v1/daemon/works', query);
  }
  getWork(workId: string): Promise<WorkDetailResponse> {
    return this.get<WorkDetailResponse>(`/v1/daemon/works/${encodeURIComponent(workId)}`);
  }
  createWork(request: CreateWorkRequest): Promise<CreateWorkResponse> {
    return this.post<CreateWorkResponse>('/v1/daemon/works', request);
  }
  patchWork(workId: string, request: PatchWorkRequest): Promise<WorkDetailResponse> {
    return this.patch<WorkDetailResponse>(
      `/v1/daemon/works/${encodeURIComponent(workId)}`,
      request,
    );
  }
  deleteWork(workId: string): Promise<void> {
    return this.delete<void>(`/v1/daemon/works/${encodeURIComponent(workId)}`);
  }

  // ── Orchestration sessions ─────────────────────────────────────────────────
  listSessions(query?: ListSessionsQuery): Promise<ListSessionsResponse> {
    return this.get<ListSessionsResponse>('/v1/daemon/orchestration/sessions', query);
  }
  getSession(sessionId: string): Promise<SessionDetailResponse> {
    return this.get<SessionDetailResponse>(
      `/v1/daemon/orchestration/sessions/${encodeURIComponent(sessionId)}`,
    );
  }

  // ── Schedules ──────────────────────────────────────────────────────────────
  listSchedules(query?: ListSchedulesQuery): Promise<ListSchedulesResponse> {
    return this.get<ListSchedulesResponse>('/v1/daemon/orchestration/schedules', query);
  }
  inspectSchedule(scheduleId: string): Promise<InspectScheduleResponse> {
    return this.get<InspectScheduleResponse>(
      `/v1/daemon/orchestration/schedules/${encodeURIComponent(scheduleId)}`,
    );
  }
  // V1.70 canvas Idea→Run/Resume/Steer promotions (V1.67 G2 pattern).
  addSchedule(request: AddScheduleRequest): Promise<AddScheduleResponse> {
    return this.post<AddScheduleResponse>('/v1/daemon/orchestration/schedules', request);
  }
  signalSchedule(
    scheduleId: string,
    request: SignalScheduleRequest,
  ): Promise<SignalScheduleResponse> {
    return this.post<SignalScheduleResponse>(
      `/v1/daemon/orchestration/schedules/${encodeURIComponent(scheduleId)}/signal`,
      request,
    );
  }
  editCoreContext(
    scheduleId: string,
    request: EditCoreContextRequest,
  ): Promise<EditCoreContextResponse> {
    return this.patch<EditCoreContextResponse>(
      `/v1/daemon/orchestration/schedules/${encodeURIComponent(scheduleId)}/core-context`,
      request,
    );
  }

  // ── Capabilities ───────────────────────────────────────────────────────────
  listCapabilities(query?: ListCapabilitiesQuery): Promise<ListCapabilitiesResponse> {
    return this.get<ListCapabilitiesResponse>('/v1/daemon/orchestration/capabilities', query);
  }

  // ── Findings ───────────────────────────────────────────────────────────────
  listFindings(workId: string, query?: ListFindingsQuery): Promise<ListFindingsResponse> {
    return this.get<ListFindingsResponse>(
      `/v1/daemon/works/${encodeURIComponent(workId)}/findings`,
      query,
    );
  }
  // V1.77 findings-remediation promotion (V1.67 G2 pattern — types + routes
  // already shipped; only the TS client surface was missing).
  getFinding(workId: string, findingId: string): Promise<FindingDetailResponse> {
    return this.get<FindingDetailResponse>(
      `/v1/daemon/works/${encodeURIComponent(workId)}/findings/${encodeURIComponent(findingId)}`,
    );
  }
  updateFinding(
    workId: string,
    findingId: string,
    patch: UpdateFindingRequest,
  ): Promise<FindingDetailResponse> {
    return this.patch<FindingDetailResponse>(
      `/v1/daemon/works/${encodeURIComponent(workId)}/findings/${encodeURIComponent(findingId)}`,
      patch,
    );
  }
  // V1.91 P1 — bulk helper for findings triage.
  batchUpdateFindings(request: BatchUpdateFindingsRequest): Promise<BatchUpdateFindingsResponse> {
    return this.patch<BatchUpdateFindingsResponse>('/v1/daemon/findings/batch', request);
  }

  // ── Preset management ──────────────────────────────────────────────────────
  listPresets(): Promise<ListPresetsResponse> {
    return this.get<ListPresetsResponse>('/v1/daemon/presets');
  }
  scaffoldPreset(request: ScaffoldPresetRequest): Promise<ScaffoldPresetResponse> {
    return this.post<ScaffoldPresetResponse>('/v1/daemon/presets', request);
  }
  validatePreset(request: ValidatePresetRequest): Promise<ValidatePresetResponse> {
    return this.post<ValidatePresetResponse>('/v1/daemon/presets:validate', request);
  }
  reloadPreset(presetId: string): Promise<ReloadPresetResponse> {
    return this.post<ReloadPresetResponse>(
      `/v1/daemon/presets/${encodeURIComponent(presetId)}:reload`,
    );
  }
  getPreset(presetId: string): Promise<GetPresetResponse> {
    return this.get<GetPresetResponse>(`/v1/daemon/presets/${encodeURIComponent(presetId)}`);
  }
  updatePreset(presetId: string, request: UpdatePresetRequest): Promise<UpdatePresetResponse> {
    return this.patch<UpdatePresetResponse>(
      `/v1/daemon/presets/${encodeURIComponent(presetId)}`,
      request,
    );
  }
  deletePreset(presetId: string): Promise<void> {
    return this.delete<void>(`/v1/daemon/presets/${encodeURIComponent(presetId)}`);
  }

  // ── Strategy canvas (V1.71 Track A) ───────────────────────────────────────
  strategyPatchState(
    strategyId: string,
    stateId: string,
    request: StrategyPatchStateRequest,
  ): Promise<StrategyPatchResponse> {
    return this.post<StrategyPatchResponse>(
      `/v1/daemon/strategies/${encodeURIComponent(strategyId)}/states/${encodeURIComponent(stateId)}/patch`,
      request,
    );
  }
  strategyPatchTransition(
    strategyId: string,
    request: StrategyPatchTransitionRequest,
  ): Promise<StrategyPatchResponse> {
    return this.post<StrategyPatchResponse>(
      `/v1/daemon/strategies/${encodeURIComponent(strategyId)}/transitions/patch`,
      request,
    );
  }
  strategyPatchPromptTemplate(
    strategyId: string,
    stateId: string,
    request: StrategyPatchPromptTemplateRequest,
  ): Promise<StrategyPatchResponse> {
    return this.post<StrategyPatchResponse>(
      `/v1/daemon/strategies/${encodeURIComponent(strategyId)}/states/${encodeURIComponent(stateId)}/prompt/patch`,
      request,
    );
  }

  // ── Chapters (V1.65 Content-Authoring) ─────────────────────────────────────
  listChapters(workId: string, query?: ListChaptersQuery): Promise<ListChaptersResponse> {
    return this.get<ListChaptersResponse>(
      `/v1/daemon/works/${encodeURIComponent(workId)}/chapters`,
      query,
    );
  }
  getChapter(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterDetail> {
    return this.get<ChapterDetail>(
      `/v1/daemon/works/${encodeURIComponent(workId)}/chapters/${chapter}`,
      query,
    );
  }
  getChapterOutline(
    workId: string,
    chapter: number,
    query?: ChapterContentQuery,
  ): Promise<ChapterOutline> {
    return this.get<ChapterOutline>(
      `/v1/daemon/works/${encodeURIComponent(workId)}/chapters/${chapter}/outline`,
      query,
    );
  }
  patchChapter(
    workId: string,
    chapter: number,
    request: PatchChapterRequest,
    query?: ChapterContentQuery,
  ): Promise<ChapterDetail> {
    return this.patch<ChapterDetail>(
      `/v1/daemon/works/${encodeURIComponent(workId)}/chapters/${chapter}`,
      request,
      query,
    );
  }
  getChapterBody(
    workId: string,
    chapter: number,
    query?: ChapterContentQuery,
  ): Promise<ChapterBody> {
    return this.get<ChapterBody>(
      `/v1/daemon/works/${encodeURIComponent(workId)}/chapters/${chapter}/body`,
      query,
    );
  }

  // ── Outline+Timeline canvas (V1.72 Track A) ────────────────────────────────
  getWorkOutline(workId: string): Promise<WorkOutline> {
    return this.get<WorkOutline>(`/v1/daemon/works/${encodeURIComponent(workId)}/outline`);
  }
  patchOutlineStructure(
    workId: string,
    request: OutlinePatchStructureRequest,
  ): Promise<OutlinePatchResponse> {
    return this.post<OutlinePatchResponse>(
      `/v1/daemon/works/${encodeURIComponent(workId)}/outline/patch`,
      request,
    );
  }
  patchOutlineChapter(
    workId: string,
    chapter: number,
    request: OutlinePatchChapterRequest,
  ): Promise<OutlinePatchResponse> {
    return this.post<OutlinePatchResponse>(
      `/v1/daemon/works/${encodeURIComponent(workId)}/chapters/${chapter}/patch`,
      request,
    );
  }
  patchTimelineEvent(
    workId: string,
    request: TimelinePatchEventRequest,
  ): Promise<OutlinePatchResponse> {
    return this.post<OutlinePatchResponse>(
      `/v1/daemon/works/${encodeURIComponent(workId)}/timeline/patch`,
      request,
    );
  }

  // ── World KB canvas (V1.73 Track A) ───────────────────────────────────────
  getWorldKbGraph(
    worldId: string,
    query?: { includeSuggested?: boolean },
  ): Promise<WorldKbGraphResponse> {
    const params = new URLSearchParams();
    if (query?.includeSuggested) params.set('include_suggested', 'true');
    const qs = params.toString();
    return this.get<WorldKbGraphResponse>(
      `/v1/daemon/worlds/${encodeURIComponent(worldId)}/kb/graph${qs ? `?${qs}` : ''}`,
    );
  }
  getWorldKbCandidates(
    worldId: string,
    query?: { limit?: number; cursor?: string },
  ): Promise<WorldKbCandidatesResponse> {
    return this.get<WorldKbCandidatesResponse>(
      `/v1/daemon/worlds/${encodeURIComponent(worldId)}/kb/candidates`,
      query,
    );
  }
  worldKbPatchEntity(
    worldId: string,
    request: WorldKbPatchEntityRequest,
  ): Promise<WorldKbPatchEntityResponse> {
    return this.post<WorldKbPatchEntityResponse>(
      `/v1/daemon/worlds/${encodeURIComponent(worldId)}/kb/patch-entity`,
      request,
    );
  }
  worldKbPromoteCandidate(
    worldId: string,
    request: WorldKbPromoteCandidateRequest,
  ): Promise<WorldKbPromoteCandidateResponse> {
    return this.post<WorldKbPromoteCandidateResponse>(
      `/v1/daemon/worlds/${encodeURIComponent(worldId)}/kb/promote-candidate`,
      request,
    );
  }
  worldKbPatchRelationship(
    worldId: string,
    request: WorldKbPatchRelationshipRequest,
  ): Promise<WorldKbPatchRelationshipResponse> {
    return this.post<WorldKbPatchRelationshipResponse>(
      `/v1/daemon/worlds/${encodeURIComponent(worldId)}/kb/patch-relationship`,
      request,
    );
  }

  // ── Compute modules (V1.114 P2) ─────────────────────────────────────────
  getComputeModules(): Promise<ListModulesResponse> {
    return this.get<ListModulesResponse>('/v1/daemon/compute/modules');
  }
  getComputeModule(moduleId: string): Promise<ModuleDetail> {
    return this.get<ModuleDetail>(`/v1/daemon/compute/modules/${encodeURIComponent(moduleId)}`);
  }
  getKeyBlockState(worldId: string, keyBlockId: string): Promise<WorldKbKeyBlockStateResponse> {
    return this.get<WorldKbKeyBlockStateResponse>(
      `/v1/daemon/worlds/${encodeURIComponent(worldId)}/kb/key-blocks/${encodeURIComponent(keyBlockId)}/state`,
    );
  }

  // ── Creator Memory review-loop (V1.78) ─────────────────────────────────────
  // Review/consume-only surface (compass D-UX LOCKED). `createPendingReview` is
  // CLI/producer-only and intentionally absent from this client. Every endpoint
  // is creator-scoped — `creator_id` rides as a query param (or body field for
  // review) and the daemon enforces active-creator ownership (403 on mismatch).
  // V1.82: workspace-scoped world list reused by the SOUL selector.
  async listNarrativeWorlds(): Promise<World[]> {
    const res = await this.get<{ worlds: World[] }>('/v1/daemon/narrative/worlds');
    return res.worlds;
  }
  deleteWorld(worldId: string): Promise<void> {
    return this.delete<void>(`/v1/daemon/worlds/${encodeURIComponent(worldId)}`);
  }
  getTimelineOverview(cursor?: string): Promise<TimelineOverviewResponse> {
    const params = cursor ? `?cursor=${encodeURIComponent(cursor)}` : '';
    return this.get<TimelineOverviewResponse>(
      `/v1/daemon/timeline/overview${params}`,
    );
  }
  listPendingReviews(
    creatorId: string,
    query?: Omit<ListPendingReviewsQuery, 'creator_id'>,
  ): Promise<ListPendingReviewsResponse> {
    return this.get<ListPendingReviewsResponse>('/v1/daemon/memory/pending-review', {
      ...query,
      creator_id: creatorId,
    });
  }
  countPendingReviews(creatorId: string): Promise<CountPendingReviewsResponse> {
    return this.get<CountPendingReviewsResponse>(
      '/v1/daemon/memory/pending-review/count',
      { creator_id: creatorId },
    );
  }
  deletePendingReview(
    pendingId: string,
    creatorId: string,
  ): Promise<DeletePendingReviewResponse> {
    return this.delete<DeletePendingReviewResponse>(
      `/v1/daemon/memory/pending-review/${encodeURIComponent(pendingId)}`,
      { creator_id: creatorId },
    );
  }
  reviewMemory(request: ReviewRequest): Promise<ReviewResponse> {
    return this.post<ReviewResponse>('/v1/daemon/memory/review', request);
  }
  listMemoryFragments(
    creatorId: string,
    query?: Omit<ListMemoryFragmentsQuery, 'creator_id'>,
  ): Promise<ListMemoryFragmentsResponse> {
    return this.get<ListMemoryFragmentsResponse>('/v1/daemon/memory/fragments', {
      ...query,
      creator_id: creatorId,
    });
  }
  reflectSoulNarrative(request: SoulNarrativeRequest): Promise<SoulNarrativeResponse> {
    return this.post<SoulNarrativeResponse>('/v1/daemon/memory/soul/reflect', request);
  }

  // ── Reading depth (V1.89) ──────────────────────────────────────────────────
  getReadingProgress(workId: string, chapter: number): Promise<ReadingProgressResponse> {
    return this.get<ReadingProgressResponse>('/v1/daemon/reading/progress', {
      work_id: workId,
      chapter,
    });
  }
  putReadingProgress(request: ReadingProgressRequest): Promise<ReadingProgressResponse> {
    return this.put<ReadingProgressResponse>('/v1/daemon/reading/progress', request);
  }
  deleteReadingProgress(workId: string, chapter: number): Promise<void> {
    return this.delete<void>('/v1/daemon/reading/progress', { work_id: workId, chapter });
  }
  listReadingAnnotations(
    workId: string,
    chapter: number,
  ): Promise<ReadingAnnotationListResponse> {
    return this.get<ReadingAnnotationListResponse>('/v1/daemon/reading/annotations', {
      work_id: workId,
      chapter,
    });
  }
  createReadingAnnotation(request: ReadingAnnotationCreateRequest): Promise<ReadingAnnotation> {
    return this.post<ReadingAnnotation>('/v1/daemon/reading/annotations', request);
  }
  patchReadingAnnotation(
    annotationId: string,
    request: ReadingAnnotationPatchRequest,
  ): Promise<ReadingAnnotation> {
    return this.patch<ReadingAnnotation>(
      `/v1/daemon/reading/annotations/${encodeURIComponent(annotationId)}`,
      request,
    );
  }
  deleteReadingAnnotation(annotationId: string): Promise<void> {
    return this.delete<void>(
      `/v1/daemon/reading/annotations/${encodeURIComponent(annotationId)}`,
    );
  }

  // ── Transport core ─────────────────────────────────────────────────────────

  private get<T>(path: string, query?: object): Promise<T> {
    return this.request<T>('GET', `${path}${toQueryString(query)}`);
  }

  private post<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('POST', path, body);
  }

  private put<T>(path: string, body: unknown): Promise<T> {
    return this.request<T>('PUT', path, body);
  }

  private patch<T>(path: string, body: unknown, query?: object): Promise<T> {
    return this.request<T>('PATCH', `${path}${toQueryString(query)}`, body);
  }

  private delete<T>(path: string, query?: object): Promise<T> {
    return this.request<T>('DELETE', `${path}${toQueryString(query)}`);
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
    includeApiKey = true,
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const init: RequestInit = { method, headers: { Accept: 'application/json' } };
    if (this.apiKey && includeApiKey) {
      init.headers = { ...init.headers, 'X-API-Key': this.apiKey };
    }
    if (body !== undefined) {
      init.headers = { ...init.headers, 'Content-Type': 'application/json' };
      init.body = JSON.stringify(body);
    }

    let response: Response;
    try {
      response = await this.fetchImpl(url, init);
    } catch (cause) {
      // Network/transport failure (daemon down, CORS, DNS, TLS, abort). The
      // legacy `transport_unreachable` `code` stays for backwards-compat with
      // tests and toast routing; `kind` carries the V1.129 P0 sub-classification
      // the dialog branches on. Classifier ordering is locked by the spec
      // (profile-create-reliability.md § Interfaces).
      throw new NexusClientError(
        0,
        'transport_unreachable',
        BrowserClient.transportMessage(this.baseUrl),
        { cause: String(cause) },
        BrowserClient.classifyTransportError(this.baseUrl, cause),
      );
    }

    // `http_fallback` (V1.129 P0): the daemon returned `text/html` with HTTP
    // 200 — the release-mode SPA fallback for an unrouted path. Treat this as
    // transport-class and re-classify before `!response.ok` would accept it as
    // a successful empty body. The body is HTML, so `response.json()` would
    // throw an unhandled SyntaxError; intercepting here keeps the failure
    // inside the NexusClientError model.
    const contentType = response.headers.get('content-type') ?? '';
    if (response.status === 200 && contentType.includes('text/html')) {
      throw new NexusClientError(
        0,
        'transport_unreachable',
        BrowserClient.transportMessage(this.baseUrl),
        { status: response.status, content_type: contentType },
        'http_fallback',
      );
    }

    if (!response.ok) {
      let errorBody: unknown = null;
      try {
        errorBody = await response.json();
      } catch {
        // Non-JSON error body; fall through to a generic status error.
      }
      throw NexusClientError.fromBody(response.status, errorBody);
    }

    // 204 No Content or empty body — resolve without parsing.
    if (response.status === 204) {
      return undefined as T;
    }
    return (await response.json()) as T;
  }

  // ── Transport classification helpers (V1.129 P0) ───────────────────────────

  /**
   * Legacy multi-cause transport message kept for toast-level backwards compat.
   * The dialog no longer renders this string directly — it branches on
   * `NexusClientError.kind` for honest per-kind copy + CTAs.
   */
  private static transportMessage(baseUrl: string): string {
    return baseUrl
      ? 'Cannot reach the daemon at this address. This usually means the URL or port is wrong, the daemon is not running, or the browser blocked a self-signed certificate. For remote daemons using self-signed certificates, use the Nexus desktop app — it can trust the certificate and store it in the OS keychain.'
      : 'Cannot reach the local daemon. Is `nexus42 daemon start` running?';
  }

  /**
   * Classify a `fetch` throw into a {@link TransportErrorKind}. Pure function
   * of `(baseUrl, cause)` so the unit-test matrix can drive each kind without a
   * running daemon.
   *
   * Locked ordering (profile-create-reliability.md § Classification algorithm):
   * 1. `baseUrl === ''` → `daemon_down`
   * 2. AbortError / AbortSignal timeout → `timeout`
   * 3. Best-effort TLS substring match → `tls` (browser hides the precise reason)
   * 4. anything else (TypeError 'Failed to fetch', unknown) → `network`
   *
   * `http_fallback` and `unknown` are not reachable from a `fetch` throw —
   * `http_fallback` requires a successful response and is detected in
   * `request()`; `unknown` is the response-path fallback when no classifier
   * matches.
   */
  private static classifyTransportError(baseUrl: string, cause: unknown): TransportErrorKind {
    // (1) Local-mode client with no remote URL configured. The fetch itself may
    // still throw a TypeError in practice; the `baseUrl` signal is the honest
    // classification regardless.
    if (!baseUrl) {
      return 'daemon_down';
    }

    // (2) Abort / timeout: explicit user abort or AbortSignal.timeout fires a
    // DOMException with name 'AbortError'.
    if (cause instanceof DOMException && cause.name === 'AbortError') {
      return 'timeout';
    }

    // (3) TLS best-effort: browsers do not expose the precise cert failure to
    // JS. Match common signal substrings; if none are present, fall through to
    // `network` rather than over-claiming cert rejection (spec TLS fail-open).
    const message =
      cause instanceof Error
        ? `${cause.name} ${cause.message}`.toLowerCase()
        : String(cause ?? '').toLowerCase();
    const TLS_SIGNALS = [
      'err_cert_authority_invalid',
      'err_cert',
      'ssl',
      'tls',
      'certificate',
    ];
    if (TLS_SIGNALS.some((signal) => message.includes(signal))) {
      return 'tls';
    }

    // (4) Default: TypeError 'Failed to fetch' or anything we cannot classify.
    return 'network';
  }
}
