/**
 * Memory family HTTP surface (P5-T2): the character-scoped memory/SOUL/ToM
 * routes and the creator-scope memory routes (P2-T2 bearer-isolated
 * authority). Every operation is a thin translation over the native family
 * surface — the stored Principal is minted natively, the per-Character
 * activity lease is admitted inside the core call, and the wire shapes are
 * the schema-owned generated DTOs (`schemas/daemon-api/characters/{memory,
 * soul,tom}/…`, `…/memory/…`).
 */
import type {
  CountPendingReviewsQuery,
  DeletePendingReviewQuery,
  CaptureCharacterPendingReviewRequest,
  CharacterSoulNarrativeRequest,
  CountCharacterPendingReviewsQuery,
  CountPendingReviewsResponse,
  DeleteCharacterPendingReviewResponse,
  DeletePendingReviewResponse,
  ListCharacterMemoryFragmentsQuery,
  ListCharacterMemoryFragmentsResponse,
  ListCharacterPendingReviewsQuery,
  ListCharacterPendingReviewsResponse,
  ListCharacterTomQuery,
  ListCharacterTomResponse,
  ListMemoryFragmentsQuery,
  ListMemoryFragmentsResponse,
  ListPendingReviewsQuery,
  ListPendingReviewsResponse,
  PromoteCharacterFragmentRequest,
  PromoteCharacterFragmentResponse,
  RecordCharacterTomRequest,
  RecordCharacterTomResponse,
  ReviewCharacterMemoryRequest,
  ReviewCharacterMemoryResponse,
  ReviewRequest,
  ReviewResponse,
  SoulNarrativeRequest,
  SoulNarrativeResponse,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { HttpError } from './errors.js';
import { parseOptionalInteger, withPrincipal, wirePayload } from './world-kb.js';

/** Family list clamp (1..=100; absent → native default 50). */
function boundedLimit(searchParams: URLSearchParams): number | undefined {
  return parseOptionalInteger(searchParams, 'limit', { min: 1, max: 100 });
}

/** `POST /v1/daemon/characters/{character_id}/memory/pending-review` — 201. */
export function captureCharacterPendingReview(
  service: ServiceCore,
  characterId: string,
  request: CaptureCharacterPendingReviewRequest,
) {
  return withPrincipal(service, (principal) =>
    service.core.captureCharacterPendingReview(principal, characterId, request),
  );
}

/** `GET /v1/daemon/characters/{character_id}/memory/pending-review`. */
export function listCharacterPendingReviews(
  service: ServiceCore,
  characterId: string,
  searchParams: URLSearchParams,
): Promise<ListCharacterPendingReviewsResponse> {
  const limit = boundedLimit(searchParams);
  const bindingId = searchParams.get('binding_id') ?? undefined;
  const cursor = searchParams.get('cursor') ?? undefined;
  const query: ListCharacterPendingReviewsQuery = {
    ...(bindingId !== undefined ? { binding_id: bindingId } : {}),
    ...(limit !== undefined ? { limit } : {}),
    ...(cursor !== undefined ? { cursor } : {}),
  };
  return withPrincipal(service, (principal) =>
    service.core.listCharacterPendingReviews(principal, characterId, query),
  );
}

/** `GET /v1/daemon/characters/{character_id}/memory/pending-review/count`. */
export function countCharacterPendingReviews(
  service: ServiceCore,
  characterId: string,
  searchParams: URLSearchParams,
) {
  const bindingId = searchParams.get('binding_id') ?? undefined;
  const query: CountCharacterPendingReviewsQuery = {
    ...(bindingId !== undefined ? { binding_id: bindingId } : {}),
  };
  return withPrincipal(service, (principal) =>
    service.core.countCharacterPendingReviews(principal, characterId, query),
  );
}

/** `DELETE /v1/daemon/characters/{character_id}/memory/pending-review/{pending_id}`. */
export function deleteCharacterPendingReview(
  service: ServiceCore,
  characterId: string,
  pendingId: string,
): Promise<DeleteCharacterPendingReviewResponse> {
  return withPrincipal(service, (principal) =>
    service.core.deleteCharacterPendingReview(principal, characterId, pendingId),
  );
}

/** `POST /v1/daemon/characters/{character_id}/memory/review`. */
export function reviewCharacterMemory(
  service: ServiceCore,
  characterId: string,
  request: ReviewCharacterMemoryRequest,
): Promise<ReviewCharacterMemoryResponse> {
  return withPrincipal(service, (principal) =>
    service.core.reviewCharacterMemory(principal, characterId, request),
  );
}

/** `GET /v1/daemon/characters/{character_id}/memory/fragments`. */
export function listCharacterMemoryFragments(
  service: ServiceCore,
  characterId: string,
  searchParams: URLSearchParams,
): Promise<ListCharacterMemoryFragmentsResponse> {
  const limit = boundedLimit(searchParams);
  const bindingId = searchParams.get('binding_id') ?? undefined;
  const cursor = searchParams.get('cursor') ?? undefined;
  const query: ListCharacterMemoryFragmentsQuery = {
    ...(bindingId !== undefined ? { binding_id: bindingId } : {}),
    ...(limit !== undefined ? { limit } : {}),
    ...(cursor !== undefined ? { cursor } : {}),
  };
  return withPrincipal(service, (principal) =>
    service.core.listCharacterMemoryFragments(principal, characterId, query),
  );
}

/** `POST /v1/daemon/characters/{character_id}/memory/fragments/{fragment_id}:promote`. */
export function promoteCharacterFragment(
  service: ServiceCore,
  characterId: string,
  fragmentId: string,
  request: PromoteCharacterFragmentRequest,
): Promise<PromoteCharacterFragmentResponse> {
  return withPrincipal(service, (principal) =>
    service.core.promoteCharacterFragment(principal, characterId, fragmentId, request),
  );
}

/** `POST /v1/daemon/characters/{character_id}/soul/reflect` — the native
 * surface has no ACP registry: observational reflects are provider-free, a
 * forced regeneration surfaces the retained 503 after authorization. */
export function reflectCharacterSoul(
  service: ServiceCore,
  characterId: string,
  request: CharacterSoulNarrativeRequest,
) {
  return withPrincipal(service, (principal) =>
    service.core.reflectCharacterSoul(principal, characterId, request),
  );
}

/** `POST /v1/daemon/characters/{character_id}/tom`. */
export function recordCharacterTom(
  service: ServiceCore,
  characterId: string,
  request: RecordCharacterTomRequest,
): Promise<RecordCharacterTomResponse> {
  return withPrincipal(service, (principal) =>
    service.core.recordCharacterTom(principal, characterId, request),
  );
}

/** `GET /v1/daemon/characters/{character_id}/tom`. */
export function listCharacterTom(
  service: ServiceCore,
  characterId: string,
  searchParams: URLSearchParams,
): Promise<ListCharacterTomResponse> {
  const limit = boundedLimit(searchParams);
  const bindingId = searchParams.get('binding_id');
  const worldId = searchParams.get('world_id');
  const cursor = searchParams.get('cursor') ?? undefined;
  if (bindingId === null || worldId === null) {
    // Required wire members: absent → 400 before any native call.
    const field = bindingId === null ? 'binding_id' : 'world_id';
    throw new HttpError(400, 'invalid_input', `${field} is required`);
  }
  const query: ListCharacterTomQuery = {
    binding_id: bindingId,
    world_id: worldId,
    ...(limit !== undefined ? { limit } : {}),
    ...(cursor !== undefined ? { cursor } : {}),
  };
  return withPrincipal(service, (principal) =>
    service.core.listCharacterTom(principal, characterId, query),
  );
}

// ── Creator-scope memory family ─────────────────────────────────────────────

/** `GET /v1/daemon/memory/pending-review`. */
export function listPendingReviews(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<ListPendingReviewsResponse> {
  const creatorId = requiredCreatorId(searchParams);
  const limit = parseOptionalInteger(searchParams, 'limit', { min: 1, max: 250 });
  const cursor = searchParams.get('cursor') ?? undefined;
  const query: ListPendingReviewsQuery = {
    creator_id: creatorId,
    ...(limit !== undefined ? { limit } : {}),
    ...(cursor !== undefined ? { cursor } : {}),
  };
  return withPrincipal(service, (principal) =>
    service.core.listPendingReviews(principal, query),
  );
}

/** `GET /v1/daemon/memory/pending-review/count?creator_id=…` — the query
 * member is required (400 when absent); the active-creator equality and
 * format checks stay native. */
export function countPendingReviews(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<CountPendingReviewsResponse> {
  const query: CountPendingReviewsQuery = { creator_id: requiredCreatorId(searchParams) };
  return withPrincipal(service, (principal) =>
    service.core.countPendingReviews(principal, query),
  );
}

/** `DELETE /v1/daemon/memory/pending-review/{pending_id}?creator_id=…` — the
 * query member is required (400 when absent); the active-creator equality
 * and format checks stay native. */
export function deletePendingReview(
  service: ServiceCore,
  pendingId: string,
  searchParams: URLSearchParams,
): Promise<DeletePendingReviewResponse> {
  const query: DeletePendingReviewQuery = { creator_id: requiredCreatorId(searchParams) };
  return withPrincipal(service, (principal) =>
    service.core.deletePendingReview(principal, pendingId, query),
  );
}

/** `POST /v1/daemon/memory/review`. */
export function reviewMemory(service: ServiceCore, request: ReviewRequest): Promise<ReviewResponse> {
  return withPrincipal(service, (principal) => service.core.reviewMemory(principal, request));
}

/** `GET /v1/daemon/memory/fragments`. */
export function listMemoryFragments(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<ListMemoryFragmentsResponse> {
  const creatorId = requiredCreatorId(searchParams);
  const keyword = searchParams.get('keyword') ?? undefined;
  const worldId = searchParams.get('world_id') ?? undefined;
  const limit = parseOptionalInteger(searchParams, 'limit', { min: 1, max: 250 });
  const query: ListMemoryFragmentsQuery = {
    creator_id: creatorId,
    ...(keyword !== undefined ? { keyword } : {}),
    ...(worldId !== undefined ? { world_id: worldId } : {}),
    ...(limit !== undefined ? { limit } : {}),
  };
  return withPrincipal(service, (principal) =>
    service.core.listMemoryFragments(principal, query),
  );
}

/** `POST /v1/daemon/memory/soul/reflect`. */
export function reflectCreatorSoul(
  service: ServiceCore,
  request: SoulNarrativeRequest,
): Promise<SoulNarrativeResponse> {
  return withPrincipal(service, (principal) =>
    service.core.reflectCreatorSoul(principal, request),
  );
}

/** `creator_id` is a required query/body member for the creator-scope routes;
 * the active-creator equality check stays native. */
function requiredCreatorId(searchParams: URLSearchParams): string {
  const creatorId = searchParams.get('creator_id');
  if (creatorId === null) {
    throw new HttpError(400, 'invalid_input', 'creator_id is required');
  }
  return creatorId;
}

/** Exact path/verb/tier identities this family owns (composer input). */
export const MEMORY_ROUTES: readonly DomainRoute[] = [
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/memory\/pending-review$/,
    tier: 'tier2',
    family: 'memory',
    status: 201,
    handle: async (service, params, _search, body) => ({
      body: await captureCharacterPendingReview(
        service,
        params[0],
        wirePayload<CaptureCharacterPendingReviewRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/memory\/pending-review$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, params, search) => ({
      body: await listCharacterPendingReviews(service, params[0], search),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/memory\/pending-review\/count$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, params, search) => ({
      body: await countCharacterPendingReviews(service, params[0], search),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/memory\/pending-review\/([^/]+)$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, params) => ({
      body: await deleteCharacterPendingReview(service, params[0], params[1]),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/memory\/review$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, params, _search, body) => ({
      body: await reviewCharacterMemory(
        service,
        params[0],
        wirePayload<ReviewCharacterMemoryRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/memory\/fragments$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, params, search) => ({
      body: await listCharacterMemoryFragments(service, params[0], search),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/memory\/fragments\/([^/]+):promote$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, params, _search, body) => ({
      body: await promoteCharacterFragment(
        service,
        params[0],
        params[1],
        wirePayload<PromoteCharacterFragmentRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/soul\/reflect$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, params, _search, body) => ({
      body: await reflectCharacterSoul(
        service,
        params[0],
        wirePayload<CharacterSoulNarrativeRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/tom$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, params, _search, body) => ({
      body: await recordCharacterTom(
        service,
        params[0],
        wirePayload<RecordCharacterTomRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/tom$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, params, search) => ({
      body: await listCharacterTom(service, params[0], search),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/memory\/pending-review$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, _params, search) => ({
      body: await listPendingReviews(service, search),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/memory\/pending-review\/count$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, _params, search) => ({
      body: await countPendingReviews(service, search),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/memory\/pending-review\/([^/]+)$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, params, search) => ({
      body: await deletePendingReview(service, params[0], search),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/memory\/review$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, _params, _search, body) => ({
      body: await reviewMemory(service, wirePayload<ReviewRequest>(body, 'request')),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/memory\/fragments$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, _params, search) => ({
      body: await listMemoryFragments(service, search),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/memory\/soul\/reflect$/,
    tier: 'tier2',
    family: 'memory',
    handle: async (service, _params, _search, body) => ({
      body: await reflectCreatorSoul(service, wirePayload<SoulNarrativeRequest>(body, 'request')),
    }),
  },
];
