/**
 * Knowledge family HTTP surface (P1-T3 authority over the single Rust core):
 * the local work-scope KB file index, Work/creator findings (including the
 * tri-state PATCH, batch triage, stale banner and retention prune), reading
 * depth (progress + annotations) and the reference-source registry reads.
 * Scope validation, enum/transition validation, batch semantics and retention
 * math stay in the core; these handlers keep transport translation only.
 */
import type {
  AddKbEntryRequest,
  AddKbEntryResponse,
  BatchUpdateFindingsRequest,
  BatchUpdateFindingsResponse,
  CreateFindingRequest,
  DeleteKbEntryResponse,
  FindingDetailResponse,
  FindingsPruneResponse,
  GetKbEntryResponse,
  ReferenceGetResponse,
  ListFindingsQuery,
  ListFindingsResponse,
  ListKbEntriesQuery,
  ListKbEntriesResponse,
  ReferenceListResponse,
  ReadingAnnotation,
  ReadingAnnotationCreateRequest,
  ReadingAnnotationListQuery,
  ReadingAnnotationListResponse,
  ReadingAnnotationPatchRequest,
  ReadingProgressQuery,
  ReadingProgressRequest,
  ReadingProgressResponse,
  StaleFindingsResponse,
  UpdateFindingRequest,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import { HttpError } from './errors.js';
import type { DomainRoute } from './routes.js';
import { parseOptionalInteger, withPrincipal, wirePayload } from './world-kb.js';

// ── Work KB index (scope=work only) ────────────────────────────────────────

/** `GET /v1/daemon/kb/entries`. */
export function listKbEntries(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<ListKbEntriesResponse> {
  const creator_id = searchParams.get('creator_id');
  const workspace_slug = searchParams.get('workspace_slug');
  const scope = searchParams.get('scope');
  const q = searchParams.get('q');
  const cursor = searchParams.get('cursor');
  const query: ListKbEntriesQuery = {
    ...(creator_id !== null ? { creator_id } : {}),
    ...(workspace_slug !== null ? { workspace_slug } : {}),
    ...(scope !== null ? { scope } : {}),
    ...(q !== null ? { q } : {}),
    ...(cursor !== null ? { cursor } : {}),
    ...boundedLimit(searchParams),
  };
  return withPrincipal(service, (principal) => service.core.listKbEntries(principal, query));
}

/** `POST /v1/daemon/kb/entries` — 201 created. */
export function addKbEntry(
  service: ServiceCore,
  request: AddKbEntryRequest,
): Promise<AddKbEntryResponse> {
  return withPrincipal(service, (principal) => service.core.addKbEntry(principal, request));
}

/** `GET /v1/daemon/kb/entries/{entry_id}`. */
export function getKbEntry(
  service: ServiceCore,
  entryId: string,
): Promise<GetKbEntryResponse> {
  return withPrincipal(service, (principal) => service.core.getKbEntry(principal, entryId));
}

/** `DELETE /v1/daemon/kb/entries/{entry_id}`. */
export function deleteKbEntry(
  service: ServiceCore,
  entryId: string,
): Promise<DeleteKbEntryResponse> {
  return withPrincipal(service, (principal) => service.core.deleteKbEntry(principal, entryId));
}

// ── Findings ───────────────────────────────────────────────────────────────

/** `POST /v1/daemon/works/{work_id}/findings` — 201 created. */
export function createFinding(
  service: ServiceCore,
  workId: string,
  request: CreateFindingRequest,
): Promise<FindingDetailResponse> {
  return withPrincipal(service, (principal) =>
    service.core.createFinding(principal, workId, request),
  );
}

/** `POST /v1/daemon/works/{work_id}/findings/from-review`. */
export function createFindingFromReview(
  service: ServiceCore,
  workId: string,
  request: CreateFindingRequest,
): Promise<FindingDetailResponse> {
  return withPrincipal(service, (principal) =>
    service.core.createFindingFromReview(principal, workId, request),
  );
}

/**
 * `GET /v1/daemon/works/{work_id}/findings` — cursor-paginated page
 * (`status` accepts a comma-separated list; the cursor is opaque).
 */
export function listFindings(
  service: ServiceCore,
  workId: string,
  searchParams: URLSearchParams,
): Promise<ListFindingsResponse> {
  const chapter = searchParams.get('chapter');
  const status = searchParams.get('status');
  const severity = searchParams.get('severity');
  const cursor = searchParams.get('cursor');
  const query: ListFindingsQuery = {
    ...(chapter !== null ? { chapter: requireInteger(chapter, 'chapter') } : {}),
    ...(status !== null ? { status } : {}),
    ...(severity !== null ? { severity } : {}),
    ...(cursor !== null ? { cursor } : {}),
    ...boundedLimit(searchParams),
  };
  return withPrincipal(service, (principal) =>
    service.core.listFindings(principal, workId, query),
  );
}

/** `GET /v1/daemon/works/{work_id}/findings/{finding_id}`. */
export function getWorkFinding(
  service: ServiceCore,
  workId: string,
  findingId: string,
): Promise<FindingDetailResponse> {
  return withPrincipal(service, (principal) =>
    service.core.getWorkFinding(principal, workId, findingId),
  );
}

/** `GET /v1/daemon/findings/{finding_id}` — creator-scoped lookup. */
export function getFinding(
  service: ServiceCore,
  findingId: string,
): Promise<FindingDetailResponse> {
  return withPrincipal(service, (principal) => service.core.getFinding(principal, findingId));
}

/**
 * `PATCH /v1/daemon/works/{work_id}/findings/{finding_id}` — tri-state
 * `rule_suggestion` (R-V1190-FINDINGS-TRISTATE-DUP): omitted keeps the stored
 * column, `null` clears it to SQL NULL, a string sets it. The three states
 * ride the unrestricted generated carrier and are enforced natively.
 */
export function updateFinding(
  service: ServiceCore,
  findingId: string,
  request: UpdateFindingWire,
): Promise<FindingDetailResponse> {
  return withPrincipal(service, (principal) =>
    service.core.updateFinding(principal, findingId, request),
  );
}

/** `DELETE /v1/daemon/works/{work_id}/findings/{finding_id}` — 204. */
export async function deleteFinding(
  service: ServiceCore,
  findingId: string,
): Promise<void> {
  await withPrincipal(service, async (principal) =>
    service.core.deleteFinding(principal, findingId),
  );
}

/** `PATCH /v1/daemon/findings/batch`. */
export function batchUpdateFindings(
  service: ServiceCore,
  request: BatchUpdateFindingsRequest,
): Promise<BatchUpdateFindingsResponse> {
  return withPrincipal(service, (principal) =>
    service.core.batchUpdateFindings(principal, request),
  );
}

/**
 * `GET /v1/daemon/findings/stale` — the stale threshold stays an adapter
 * concern (daemon env default 96h), matching the legacy surface.
 */
export function listStaleFindings(service: ServiceCore): Promise<StaleFindingsResponse> {
  return withPrincipal(service, (principal) =>
    service.core.listStaleFindings(principal, staleThresholdSeconds()),
  );
}

/** `POST /v1/daemon/findings/prune?older_than_days=&dry_run=`. */
export function pruneFindings(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<FindingsPruneResponse> {
  const older_than_days = parseOptionalInteger(searchParams, 'older_than_days', { min: 0 });
  const dry_run = searchParams.get('dry_run');
  if (dry_run !== null && dry_run !== 'true' && dry_run !== 'false') {
    throw new HttpError(
      400,
      'invalid_input',
      'dry_run must be a boolean',
    );
  }
  return withPrincipal(service, (principal) =>
    service.core.pruneFindings(
      principal,
      older_than_days ?? null,
      dry_run === 'true',
    ),
  );
}

// ── Reading depth ──────────────────────────────────────────────────────────

/** `GET /v1/daemon/reading/progress?work_id=&chapter=`. */
export function getReadingProgress(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<ReadingProgressResponse> {
  return withPrincipal(service, (principal) =>
    service.core.getReadingProgress(principal, readingQuery(searchParams)),
  );
}

/** `PUT /v1/daemon/reading/progress`. */
export function putReadingProgress(
  service: ServiceCore,
  request: ReadingProgressRequest,
): Promise<ReadingProgressResponse> {
  return withPrincipal(service, (principal) =>
    service.core.putReadingProgress(principal, request.work_id, request),
  );
}

/** `DELETE /v1/daemon/reading/progress?work_id=&chapter=` — 204. */
export async function deleteReadingProgress(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<void> {
  await withPrincipal(service, async (principal) =>
    service.core.deleteReadingProgress(principal, readingQuery(searchParams)),
  );
}

/** `GET /v1/daemon/reading/annotations?work_id=&chapter=`. */
export function listAnnotations(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<ReadingAnnotationListResponse> {
  return withPrincipal(service, (principal) =>
    service.core.listAnnotations(principal, readingQuery(searchParams)),
  );
}

/** `POST /v1/daemon/reading/annotations` — 201 created. */
export function createAnnotation(
  service: ServiceCore,
  request: ReadingAnnotationCreateRequest,
): Promise<ReadingAnnotation> {
  return withPrincipal(service, (principal) =>
    service.core.createAnnotation(principal, request),
  );
}

/** `PATCH /v1/daemon/reading/annotations/{annotation_id}` — explicit nullable `note`. */
export function patchAnnotation(
  service: ServiceCore,
  annotationId: string,
  request: ReadingAnnotationPatchRequest,
): Promise<ReadingAnnotation> {
  return withPrincipal(service, (principal) =>
    service.core.patchAnnotation(principal, annotationId, request),
  );
}

/** `DELETE /v1/daemon/reading/annotations/{annotation_id}` — 204. */
export async function deleteAnnotation(
  service: ServiceCore,
  annotationId: string,
): Promise<void> {
  await withPrincipal(service, async (principal) =>
    service.core.deleteAnnotation(principal, annotationId),
  );
}

// ── Reference registry ─────────────────────────────────────────────────────

/** `GET /v1/daemon/references`. */
export function listReferences(service: ServiceCore): Promise<ReferenceListResponse> {
  return withPrincipal(service, (principal) => service.core.listReferences(principal));
}

/** `GET /v1/daemon/references/{reference_id}` — 404 for an unknown id. */
export function getReference(
  service: ServiceCore,
  referenceId: string,
): Promise<ReferenceGetResponse> {
  return withPrincipal(service, (principal) =>
    service.core.getReference(principal, referenceId),
  );
}

/** Exact path/verb/tier identities this family owns (composer input). */
export const KNOWLEDGE_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/kb\/entries$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, _params, search) => ({ body: await listKbEntries(service, search) }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/kb\/entries$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, _params, _search, body) => ({
      body: await addKbEntry(service, wirePayload<AddKbEntryRequest>(body, 'request')),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/kb\/entries\/([^/]+)$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, params) => ({ body: await getKbEntry(service, params[0]) }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/kb\/entries\/([^/]+)$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, params) => ({ body: await deleteKbEntry(service, params[0]) }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/findings$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, params, search) => ({
      body: await listFindings(service, params[0], search),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/findings\/from-review$/,
    tier: 'tier2',
    family: 'knowledge',
    status: 201,
    handle: async (service, params, _search, body) => ({
      body: await createFindingFromReview(
        service,
        params[0],
        wirePayload<CreateFindingRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/findings$/,
    tier: 'tier2',
    family: 'knowledge',
    status: 201,
    handle: async (service, params, _search, body) => ({
      body: await createFinding(
        service,
        params[0],
        wirePayload<CreateFindingRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/findings\/([^/]+)$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, params) => ({
      body: await getWorkFinding(service, params[0], params[1]),
    }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/findings\/([^/]+)$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, params, _search, body) => ({
      body: await updateFinding(
        service,
        params[1],
        wirePayload<UpdateFindingWire>(body, 'request'),
      ),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/findings\/([^/]+)$/,
    tier: 'tier2',
    family: 'knowledge',
    status: 204,
    handle: async (service, params) => {
      await deleteFinding(service, params[1]);
      return { body: null };
    },
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/findings\/stale$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service) => ({ body: await listStaleFindings(service) }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/findings\/batch$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, _params, _search, body) => ({
      body: await batchUpdateFindings(
        service,
        wirePayload<BatchUpdateFindingsRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/findings\/prune$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, _params, search) => ({ body: await pruneFindings(service, search) }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/findings\/([^/]+)$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, params) => ({ body: await getFinding(service, params[0]) }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/reading\/progress$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, _params, search) => ({
      body: await getReadingProgress(service, search),
    }),
  },
  {
    method: 'PUT',
    pattern: /^\/v1\/daemon\/reading\/progress$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, _params, _search, body) => ({
      body: await putReadingProgress(
        service,
        wirePayload<ReadingProgressRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/reading\/progress$/,
    tier: 'tier2',
    family: 'knowledge',
    status: 204,
    handle: async (service, _params, search) => {
      await deleteReadingProgress(service, search);
      return { body: null };
    },
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/reading\/annotations$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, _params, search) => ({
      body: await listAnnotations(service, search),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/reading\/annotations$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, _params, _search, body) => ({
      body: await createAnnotation(
        service,
        wirePayload<ReadingAnnotationCreateRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/reading\/annotations\/([^/]+)$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, params, _search, body) => ({
      body: await patchAnnotation(
        service,
        params[0],
        wirePayload<ReadingAnnotationPatchRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/reading\/annotations\/([^/]+)$/,
    tier: 'tier2',
    family: 'knowledge',
    status: 204,
    handle: async (service, params) => {
      await deleteAnnotation(service, params[0]);
      return { body: null };
    },
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/references$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service) => ({ body: await listReferences(service) }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/references\/([^/]+)$/,
    tier: 'tier2',
    family: 'knowledge',
    handle: async (service, params) => ({ body: await getReference(service, params[0]) }),
  },
];

// ── local grammar helpers ──────────────────────────────────────────────────

/** The tri-state PATCH wire carrier (`rule_suggestion` stays unrestricted). */
export type UpdateFindingWire = Omit<UpdateFindingRequest, 'rule_suggestion'> & {
  rule_suggestion?: unknown;
};

/** Bounded `limit` (1..100) for the offset/cursor pages this family serves. */
function boundedLimit(searchParams: URLSearchParams): { limit?: number } {
  const raw = searchParams.get('limit');
  if (raw === null) return {};
  const parsed = requireInteger(raw, 'limit');
  if (parsed < 1 || parsed > 100) {
    throw new HttpError(
      400,
      'invalid_input',
      'limit must be an integer between 1 and 100',
    );
  }
  return { limit: parsed };
}

/** Required non-negative integer query parameter. */
function requireInteger(raw: string, field: string): number {
  if (!/^\d+$/.test(raw)) {
    throw new HttpError(400, 'invalid_input', `${field} must be an integer`);
  }
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isSafeInteger(parsed)) {
    throw new HttpError(
      400,
      'invalid_input',
      `${field} must be a safe integer`,
    );
  }
  return parsed;
}

function readingQuery(searchParams: URLSearchParams): ReadingProgressQuery {
  const work_id = searchParams.get('work_id');
  const chapter = searchParams.get('chapter');
  if (work_id === null || chapter === null) {
    throw new HttpError(
      400,
      'invalid_input',
      'work_id and chapter are required',
    );
  }
  return { work_id, chapter: requireInteger(chapter, 'chapter') };
}

/** Retained daemon default for the stale banner (96h in seconds). */
const DEFAULT_STALE_THRESHOLD_SECONDS = 96 * 60 * 60;

function staleThresholdSeconds(): number {
  const raw = process.env.NEXUS_DAEMON_STALE_FINDINGS_THRESHOLD_SECS;
  const parsed = raw === undefined ? Number.NaN : Number.parseInt(raw, 10);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : DEFAULT_STALE_THRESHOLD_SECONDS;
}
