/**
 * Content family HTTP surface (P5-T1): Work chapter/manuscript content and
 * outline/timeline structure patches (P1-T2 authority). Path arguments stay
 * explicit route parameters, `ChapterContentQuery` stays a query (never a
 * body), per-Work locks, published/finalized rules, hash/CAS and path/symlink
 * confinement stay in the single Rust core authority, and the wire shapes are
 * the schema-owned generated DTOs (`schemas/daemon-api/works/chapters/…`,
 * `schemas/daemon-api/canvas/outline/…`).
 */
import type {
  ChapterBody,
  ChapterContentQuery,
  ChapterDetail,
  ChapterOutline,
  ListChaptersQuery,
  ListChaptersResponse,
  OutlinePatchChapterRequest,
  OutlinePatchResponse,
  OutlinePatchStructureRequest,
  PatchChapterRequest,
  TimelinePatchEventRequest,
  WorkOutline,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { parseOptionalInteger, withPrincipal, wirePayload } from './world-kb.js';

/** Chapter content query from `?volume=` (a present volume must be non-zero). */
export function chapterQuery(searchParams: URLSearchParams): ChapterContentQuery {
  const volume = parseOptionalInteger(searchParams, 'volume', { min: 1 });
  return volume === undefined ? {} : { volume };
}

/** `GET /v1/daemon/works/{work_id}/chapters/` — cursor-paginated summaries. */
export function listChapters(
  service: ServiceCore,
  workId: string,
  searchParams: URLSearchParams,
): Promise<ListChaptersResponse> {
  const status = searchParams.get('status');
  const cursor = searchParams.get('cursor');
  const limit = searchParams.get('limit');
  const query: ListChaptersQuery = {
    ...(status !== null ? { status } : {}),
    ...(cursor !== null ? { cursor } : {}),
    ...(limit !== null ? { limit: parseOptionalInteger(searchParams, 'limit', { min: 1 }) } : {}),
  };
  return withPrincipal(service, (principal) =>
    service.core.listChapters(principal, workId, query),
  );
}

/** `GET /v1/daemon/works/{work_id}/chapters/{n}` — chapter detail. */
export function getChapter(
  service: ServiceCore,
  workId: string,
  chapter: string,
  searchParams: URLSearchParams,
): Promise<ChapterDetail> {
  return withPrincipal(service, (principal) =>
    service.core.chapterDetail(principal, workId, chapter, chapterQuery(searchParams)),
  );
}

/** `GET /v1/daemon/works/{work_id}/chapters/{n}/outline` — outline markdown. */
export function getChapterOutline(
  service: ServiceCore,
  workId: string,
  chapter: string,
  searchParams: URLSearchParams,
): Promise<ChapterOutline> {
  return withPrincipal(service, (principal) =>
    service.core.chapterOutline(principal, workId, chapter, chapterQuery(searchParams)),
  );
}

/**
 * `GET /v1/daemon/works/{work_id}/chapters/{n}/body` — the retained raw
 * content download: markdown inside the schema-owned `ChapterBody` envelope.
 */
export function getChapterBody(
  service: ServiceCore,
  workId: string,
  chapter: string,
  searchParams: URLSearchParams,
): Promise<ChapterBody> {
  return withPrincipal(service, (principal) =>
    service.core.chapterBody(principal, workId, chapter, chapterQuery(searchParams)),
  );
}

/** `PATCH /v1/daemon/works/{work_id}/chapters/{n}` — partial structure update. */
export function patchChapter(
  service: ServiceCore,
  workId: string,
  chapter: string,
  searchParams: URLSearchParams,
  request: PatchChapterRequest,
): Promise<ChapterDetail> {
  return withPrincipal(service, (principal) =>
    service.core.patchChapter(principal, workId, chapter, chapterQuery(searchParams), request),
  );
}

/** `GET /v1/daemon/works/{work_id}/outline` — canonical outline + timeline. */
export function getWorkOutline(service: ServiceCore, workId: string): Promise<WorkOutline> {
  return withPrincipal(service, (principal) => service.core.getWorkOutline(principal, workId));
}

/** `POST /v1/daemon/works/{work_id}/outline/patch` — structured outline patch. */
export function patchOutlineStructure(
  service: ServiceCore,
  workId: string,
  request: OutlinePatchStructureRequest,
): Promise<OutlinePatchResponse> {
  return withPrincipal(service, (principal) =>
    service.core.patchOutlineStructure(principal, workId, request),
  );
}

/** `POST /v1/daemon/works/{work_id}/chapters/{n}/patch` — outline chapter patch. */
export function patchOutlineChapter(
  service: ServiceCore,
  workId: string,
  chapter: string,
  request: OutlinePatchChapterRequest,
): Promise<OutlinePatchResponse> {
  return withPrincipal(service, (principal) =>
    service.core.patchOutlineChapter(principal, workId, chapter, request),
  );
}

/** `POST /v1/daemon/works/{work_id}/timeline/patch` — structured timeline patch. */
export function patchTimelineEvent(
  service: ServiceCore,
  workId: string,
  request: TimelinePatchEventRequest,
): Promise<OutlinePatchResponse> {
  return withPrincipal(service, (principal) =>
    service.core.patchTimelineEvent(principal, workId, request),
  );
}


/** Exact path/verb/tier identities this family owns (composer input). */
export const CONTENT_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/chapters\/$/,
    tier: 'tier2',
    family: 'content',
    handle: async (service, params, search) => ({
      body: await listChapters(service, params[0], search),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/chapters\/([^/]+)\/outline$/,
    tier: 'tier2',
    family: 'content',
    handle: async (service, params, search) => ({
      body: await getChapterOutline(service, params[0], params[1], search),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/chapters\/([^/]+)\/body$/,
    tier: 'tier2',
    family: 'content',
    handle: async (service, params, search) => ({
      body: await getChapterBody(service, params[0], params[1], search),
    }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/chapters\/([^/]+)$/,
    tier: 'tier2',
    family: 'content',
    handle: async (service, params, search, body) => ({
      body: await patchChapter(
        service,
        params[0],
        params[1],
        search,
        wirePayload<PatchChapterRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/outline$/,
    tier: 'tier2',
    family: 'content',
    handle: async (service, params) => ({ body: await getWorkOutline(service, params[0]) }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/outline\/patch$/,
    tier: 'tier2',
    family: 'content',
    handle: async (service, params, _search, body) => ({
      body: await patchOutlineStructure(
        service,
        params[0],
        wirePayload<OutlinePatchStructureRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/chapters\/([^/]+)\/patch$/,
    tier: 'tier2',
    family: 'content',
    handle: async (service, params, _search, body) => ({
      body: await patchOutlineChapter(
        service,
        params[0],
        params[1],
        wirePayload<OutlinePatchChapterRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/timeline\/patch$/,
    tier: 'tier2',
    family: 'content',
    handle: async (service, params, _search, body) => ({
      body: await patchTimelineEvent(
        service,
        params[0],
        wirePayload<TimelinePatchEventRequest>(body, 'request'),
      ),
    }),
  },
];

