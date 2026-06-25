/**
 * `BrowserClient` — the V1.64 NexusClient implementation for the browser.
 *
 * Spec: web-ui.md §5. Uses `fetch` against same-origin `/v1/local/*`. In dev
 * the Vite dev server proxies these requests to the running daemon
 * (vite.config.ts, default http://127.0.0.1:8420); in release the daemon serves
 * the embedded SPA at `/` and the Local API at `/v1/local/*` on the same port.
 *
 * The daemon's Local API data endpoints are keyless on loopback (V1.20 model),
 * so this client sends no credentials.
 */
import type {
  ChapterBody,
  ChapterContentQuery,
  ChapterDetail,
  ChapterOutline,
  CreateWorkRequest,
  CreateWorkResponse,
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

import { NexusClientError } from './errors';
import type { DaemonHealth, NexusClient } from './types';

export interface BrowserClientOptions {
  /**
   * Origin/base path prefix for Local API requests. Defaults to `''` (same
   * origin, relative). Set only if the API is served from a different origin
   * than the SPA shell.
   */
  baseUrl?: string;
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
  private readonly fetchImpl: typeof fetch;

  constructor(options: BrowserClientOptions = {}) {
    this.baseUrl = (options.baseUrl ?? '').replace(/\/+$/, '');
    this.fetchImpl = options.fetchImpl ?? fetch.bind(globalThis);
  }

  // ── Daemon ─────────────────────────────────────────────────────────────────
  health(): Promise<DaemonHealth> {
    return this.get<DaemonHealth>('/v1/local/runtime/health');
  }

  // ── Works ──────────────────────────────────────────────────────────────────
  listWorks(query?: ListWorksQuery): Promise<ListWorksResponse> {
    return this.get<ListWorksResponse>('/v1/local/works', query);
  }
  getWork(workId: string): Promise<WorkDetailResponse> {
    return this.get<WorkDetailResponse>(`/v1/local/works/${encodeURIComponent(workId)}`);
  }
  createWork(request: CreateWorkRequest): Promise<CreateWorkResponse> {
    return this.post<CreateWorkResponse>('/v1/local/works', request);
  }
  patchWork(workId: string, request: PatchWorkRequest): Promise<WorkDetailResponse> {
    return this.patch<WorkDetailResponse>(
      `/v1/local/works/${encodeURIComponent(workId)}`,
      request,
    );
  }

  // ── Orchestration sessions ─────────────────────────────────────────────────
  listSessions(query?: ListSessionsQuery): Promise<ListSessionsResponse> {
    return this.get<ListSessionsResponse>('/v1/local/orchestration/sessions', query);
  }
  getSession(sessionId: string): Promise<SessionDetailResponse> {
    return this.get<SessionDetailResponse>(
      `/v1/local/orchestration/sessions/${encodeURIComponent(sessionId)}`,
    );
  }

  // ── Schedules ──────────────────────────────────────────────────────────────
  listSchedules(query?: ListSchedulesQuery): Promise<ListSchedulesResponse> {
    return this.get<ListSchedulesResponse>('/v1/local/orchestration/schedules', query);
  }
  inspectSchedule(scheduleId: string): Promise<InspectScheduleResponse> {
    return this.get<InspectScheduleResponse>(
      `/v1/local/orchestration/schedules/${encodeURIComponent(scheduleId)}`,
    );
  }

  // ── Capabilities ───────────────────────────────────────────────────────────
  listCapabilities(): Promise<ListCapabilitiesResponse> {
    return this.get<ListCapabilitiesResponse>('/v1/local/orchestration/capabilities');
  }

  // ── Findings ───────────────────────────────────────────────────────────────
  listFindings(workId: string, query?: ListFindingsQuery): Promise<ListFindingsResponse> {
    return this.get<ListFindingsResponse>(
      `/v1/local/works/${encodeURIComponent(workId)}/findings`,
      query,
    );
  }

  // ── Preset management ──────────────────────────────────────────────────────
  listPresets(): Promise<ListPresetsResponse> {
    return this.get<ListPresetsResponse>('/v1/local/presets');
  }
  scaffoldPreset(request: ScaffoldPresetRequest): Promise<ScaffoldPresetResponse> {
    return this.post<ScaffoldPresetResponse>('/v1/local/presets', request);
  }
  validatePreset(request: ValidatePresetRequest): Promise<ValidatePresetResponse> {
    return this.post<ValidatePresetResponse>('/v1/local/presets:validate', request);
  }
  reloadPreset(presetId: string): Promise<ReloadPresetResponse> {
    return this.post<ReloadPresetResponse>(
      `/v1/local/presets/${encodeURIComponent(presetId)}:reload`,
    );
  }

  // ── Chapters (V1.65 Content-Authoring) ─────────────────────────────────────
  listChapters(workId: string, query?: ListChaptersQuery): Promise<ListChaptersResponse> {
    return this.get<ListChaptersResponse>(
      `/v1/local/works/${encodeURIComponent(workId)}/chapters`,
      query,
    );
  }
  getChapter(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterDetail> {
    return this.get<ChapterDetail>(
      `/v1/local/works/${encodeURIComponent(workId)}/chapters/${chapter}`,
      query,
    );
  }
  getChapterOutline(
    workId: string,
    chapter: number,
    query?: ChapterContentQuery,
  ): Promise<ChapterOutline> {
    return this.get<ChapterOutline>(
      `/v1/local/works/${encodeURIComponent(workId)}/chapters/${chapter}/outline`,
      query,
    );
  }
  putChapterOutline(
    workId: string,
    chapter: number,
    request: PutChapterOutlineRequest,
    query?: ChapterContentQuery,
  ): Promise<ChapterOutline> {
    return this.put<ChapterOutline>(
      `/v1/local/works/${encodeURIComponent(workId)}/chapters/${chapter}/outline`,
      request,
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
      `/v1/local/works/${encodeURIComponent(workId)}/chapters/${chapter}`,
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
      `/v1/local/works/${encodeURIComponent(workId)}/chapters/${chapter}/body`,
      query,
    );
  }

  // ── Transport core ─────────────────────────────────────────────────────────

  private get<T>(path: string, query?: object): Promise<T> {
    return this.request<T>('GET', `${path}${toQueryString(query)}`);
  }

  private post<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('POST', path, body);
  }

  private patch<T>(path: string, body: unknown, query?: object): Promise<T> {
    return this.request<T>('PATCH', `${path}${toQueryString(query)}`, body);
  }

  private put<T>(path: string, body: unknown, query?: object): Promise<T> {
    return this.request<T>('PUT', `${path}${toQueryString(query)}`, body);
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const init: RequestInit = { method, headers: { Accept: 'application/json' } };
    if (body !== undefined) {
      init.headers = { ...init.headers, 'Content-Type': 'application/json' };
      init.body = JSON.stringify(body);
    }

    let response: Response;
    try {
      response = await this.fetchImpl(url, init);
    } catch (cause) {
      // Network/transport failure (daemon down, CORS, DNS). The toast layer
      // surfaces `message`; `code` distinguishes it from an HTTP error.
      throw new NexusClientError(
        0,
        'transport_unreachable',
        'Cannot reach the local daemon. Is `nexus42 daemon start` running?',
        { cause: String(cause) },
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
}
