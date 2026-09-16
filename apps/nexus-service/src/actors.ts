/**
 * Actor family HTTP surface (P5-T2): Character identity and bindings (P2-T1
 * authority), the admitted-actor KnowledgeView, and the Creator home family.
 * Every operation is a thin translation over the native family surface — the
 * stored Principal is minted natively, ownership/CAS/effects stay in the
 * single Rust core authority, and the wire shapes are the schema-owned
 * generated DTOs (`schemas/daemon-api/characters/…`, `…/actor-knowledge/…`,
 * `…/creators/…`).
 */
import type {
  AddCharacterBindingRequest,
  ListCharactersResponse,
  AddKnowledgeEntryRequest,
  CharacterDetail,
  CharacterLifecycleRequest,
  CreateCharacterRequest,
  CreateCharacterResponse,
  CreatorDetail,
  ListCharacterBindingsQuery,
  ListCharacterKnowledgeQuery,
  ListCharactersQuery,
  ListCreatorsQuery,
  UpdateCharacterBindingRequest,
  UpdateCharacterRequest,
  UpdateKnowledgeEntryRequest,
  ViewRequest,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { HttpError } from './errors.js';
import { parseOptionalInteger, withPrincipal, wirePayload } from './world-kb.js';

/** `GET /v1/daemon/characters`. */
export function listCharacters(
  service: ServiceCore,
  searchParams: URLSearchParams,
) {
  const limit = boundedLimit(searchParams);
  const cursor = searchParams.get('cursor') ?? undefined;
  const query: ListCharactersQuery = {
    ...(limit !== undefined ? { limit } : {}),
    ...(cursor !== undefined ? { cursor } : {}),
  };
  return withPrincipal(
    service,
    (principal): Promise<ListCharactersResponse> =>
      service.core.listCharacters(principal, query),
  );
}

/** `POST /v1/daemon/characters` — 201 created. */
export function createCharacter(
  service: ServiceCore,
  request: CreateCharacterRequest,
): Promise<CreateCharacterResponse> {
  return withPrincipal(service, (principal) => service.core.createCharacter(principal, request));
}

/** `GET /v1/daemon/characters/{character_id}` — a foreign Character is 404. */
export function getCharacter(service: ServiceCore, characterId: string): Promise<CharacterDetail> {
  return withPrincipal(service, (principal) => service.core.getCharacter(principal, characterId));
}

/** `PATCH /v1/daemon/characters/{character_id}` — tri-state patch in native. */
export function patchCharacter(
  service: ServiceCore,
  characterId: string,
  request: UpdateCharacterRequest,
): Promise<CharacterDetail> {
  return withPrincipal(service, (principal) =>
    service.core.patchCharacter(principal, characterId, request),
  );
}

/** `POST /v1/daemon/characters/{character_id}/archive`. */
export function archiveCharacter(
  service: ServiceCore,
  characterId: string,
  request: CharacterLifecycleRequest,
): Promise<CharacterDetail> {
  return withPrincipal(service, (principal) =>
    service.core.archiveCharacter(principal, characterId, request),
  );
}

/** `POST /v1/daemon/characters/{character_id}/restore`. */
export function restoreCharacter(
  service: ServiceCore,
  characterId: string,
  request: CharacterLifecycleRequest,
): Promise<CharacterDetail> {
  return withPrincipal(service, (principal) =>
    service.core.restoreCharacter(principal, characterId, request),
  );
}

/** `POST /v1/daemon/characters/{character_id}/bindings` — 201 created. */
export function addCharacterBinding(
  service: ServiceCore,
  characterId: string,
  request: AddCharacterBindingRequest,
) {
  return withPrincipal(service, (principal) =>
    service.core.addCharacterBinding(principal, characterId, request),
  );
}

/** `GET /v1/daemon/characters/{character_id}/bindings`. */
export function listCharacterBindings(
  service: ServiceCore,
  characterId: string,
  searchParams: URLSearchParams,
) {
  const limit = boundedLimit(searchParams);
  const cursor = searchParams.get('cursor') ?? undefined;
  const query: ListCharacterBindingsQuery = {
    ...(limit !== undefined ? { limit } : {}),
    ...(cursor !== undefined ? { cursor } : {}),
  };
  return withPrincipal(service, (principal) =>
    service.core.listCharacterBindings(principal, characterId, query),
  );
}

/** `GET /v1/daemon/characters/{character_id}/bindings/{binding_id}`. */
export function getCharacterBinding(
  service: ServiceCore,
  characterId: string,
  bindingId: string,
) {
  return withPrincipal(service, (principal) =>
    service.core.getCharacterBinding(principal, characterId, bindingId),
  );
}

/** `PATCH /v1/daemon/characters/{character_id}/bindings/{binding_id}` — stale
 * `expected_revision` is the native 409, never a silent overwrite. */
export function patchCharacterBinding(
  service: ServiceCore,
  characterId: string,
  bindingId: string,
  request: UpdateCharacterBindingRequest,
) {
  return withPrincipal(service, (principal) =>
    service.core.patchCharacterBinding(principal, characterId, bindingId, request),
  );
}

/** `DELETE /v1/daemon/characters/{character_id}/bindings/{binding_id}` — 204. */
export async function removeCharacterBinding(
  service: ServiceCore,
  characterId: string,
  bindingId: string,
): Promise<void> {
  await withPrincipal(service, (principal) =>
    service.core.removeCharacterBinding(principal, characterId, bindingId),
  );
}

// ── Actor KnowledgeView ─────────────────────────────────────────────────────

/** `POST /v1/daemon/actor-knowledge/view` — the admitted-actor context read. */
export function actorKnowledgeView(service: ServiceCore, request: ViewRequest) {
  return withPrincipal(service, (principal) =>
    service.core.actorKnowledgeView(principal, request),
  );
}

/** `POST /v1/daemon/actor-knowledge/entries` — 201 created. */
export function addActorKnowledgeEntry(service: ServiceCore, request: AddKnowledgeEntryRequest) {
  return withPrincipal(service, (principal) =>
    service.core.addActorKnowledgeEntry(principal, request),
  );
}

/** `GET /v1/daemon/characters/{character_id}/knowledge`. */
export function listCharacterKnowledge(
  service: ServiceCore,
  characterId: string,
  searchParams: URLSearchParams,
) {
  const limit = boundedLimit(searchParams);
  const cursor = searchParams.get('cursor') ?? undefined;
  const query: ListCharacterKnowledgeQuery = {
    ...(limit !== undefined ? { limit } : {}),
    ...(cursor !== undefined ? { cursor } : {}),
  };
  return withPrincipal(service, (principal) =>
    service.core.listCharacterKnowledge(principal, characterId, query),
  );
}

/** `GET /v1/daemon/characters/{character_id}/knowledge/{entry_id}`. */
export function getKnowledgeEntry(
  service: ServiceCore,
  characterId: string,
  entryId: string,
) {
  return withPrincipal(service, (principal) =>
    service.core.getKnowledgeEntry(principal, characterId, entryId),
  );
}

/** `PATCH /v1/daemon/characters/{character_id}/knowledge/{entry_id}`. */
export function patchKnowledgeEntry(
  service: ServiceCore,
  characterId: string,
  entryId: string,
  request: UpdateKnowledgeEntryRequest,
) {
  return withPrincipal(service, (principal) =>
    service.core.patchKnowledgeEntry(principal, characterId, entryId, request),
  );
}

/** `DELETE /v1/daemon/characters/{character_id}/knowledge/{entry_id}` — 204;
 * `expected_revision` is a required query parameter. */
export async function deleteKnowledgeEntry(
  service: ServiceCore,
  characterId: string,
  entryId: string,
  searchParams: URLSearchParams,
): Promise<void> {
  const raw = searchParams.get('expected_revision');
  if (raw === null) {
    throw new HttpError(400, 'invalid_input', 'expected_revision is required');
  }
  const parsed = Number(raw);
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    throw new HttpError(400, 'invalid_input', 'expected_revision must be an integer');
  }
  await withPrincipal(service, (principal) =>
    service.core.deleteKnowledgeEntry(principal, characterId, entryId, parsed),
  );
}

// ── Creator identity family (Tier-1: no active-creator requirement) ────────

/** `GET /v1/daemon/creators`. */
export function listCreators(service: ServiceCore, searchParams: URLSearchParams) {
  const limit = boundedLimit(searchParams);
  const cursor = searchParams.get('cursor') ?? undefined;
  const query: ListCreatorsQuery = {
    ...(limit !== undefined ? { limit } : {}),
    ...(cursor !== undefined ? { cursor } : {}),
  };
  return service.core.listCreators(query);
}

/** `POST /v1/daemon/creators` — 201 created. */
export function createCreator(service: ServiceCore, body: unknown) {
  const request = wirePayload<{ display_name?: unknown }>(body, 'request');
  if (typeof request.display_name !== 'string') {
    throw new HttpError(400, 'invalid_input', 'display_name must be a string');
  }
  return service.core.createCreator(request.display_name);
}

/** `GET /v1/daemon/creators/{creator_id}`. */
export function getCreator(service: ServiceCore, creatorId: string): Promise<CreatorDetail> {
  return service.core.getCreator(creatorId);
}

/** `PATCH /v1/daemon/creators/{creator_id}`. */
export function patchCreator(service: ServiceCore, creatorId: string, body: unknown) {
  const request = wirePayload<{ display_name?: unknown }>(body, 'request');
  if (request.display_name !== undefined && typeof request.display_name !== 'string') {
    throw new HttpError(400, 'invalid_input', 'display_name must be a string');
  }
  return service.core.patchCreator(creatorId, request.display_name as string | undefined);
}

/** `PUT /v1/daemon/creators/active`. */
export function setActiveCreator(service: ServiceCore, body: unknown) {
  return service.core.setActiveCreator(wirePayload(body, 'request'));
}

/** `GET /v1/daemon/creators/active`. */
export function getActiveCreator(service: ServiceCore) {
  return service.core.getActiveCreator();
}

/** `POST /v1/daemon/creators/{creator_id}:logout` — the retained verb rides
 * the shared `{creator_id}` segment (`matchit` rejects `:a:b` patterns), so
 * the `:logout` suffix is stripped here exactly like the daemon handler; a
 * POST without the suffix is not a routed identity (404). */
export function logoutCreator(service: ServiceCore, segment: string) {
  const creatorId = segment.replace(/:logout$/, '');
  if (creatorId === segment) {
    throw new HttpError(404, 'not_found', `Creator route '${segment}' not found`);
  }
  return service.core.logoutCreator(creatorId);
}

/** Family list clamp shared with the native surface (1..=100; absent → native default). */
function boundedLimit(searchParams: URLSearchParams): number | undefined {
  return parseOptionalInteger(searchParams, 'limit', { min: 1, max: 100 });
}

/** Exact path/verb/tier identities this family owns (composer input). */
export const ACTOR_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/characters$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, _params, search) => ({ body: await listCharacters(service, search) }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/characters$/,
    tier: 'tier2',
    family: 'actors',
    status: 201,
    handle: async (service, _params, _search, body) => ({
      body: await createCharacter(service, wirePayload<CreateCharacterRequest>(body, 'request')),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, params) => ({ body: await getCharacter(service, params[0]) }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, params, _search, body) => ({
      body: await patchCharacter(
        service,
        params[0],
        wirePayload<UpdateCharacterRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/archive$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, params, _search, body) => ({
      body: await archiveCharacter(
        service,
        params[0],
        wirePayload<CharacterLifecycleRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/restore$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, params, _search, body) => ({
      body: await restoreCharacter(
        service,
        params[0],
        wirePayload<CharacterLifecycleRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/bindings$/,
    tier: 'tier2',
    family: 'actors',
    status: 201,
    handle: async (service, params, _search, body) => ({
      body: await addCharacterBinding(
        service,
        params[0],
        wirePayload<AddCharacterBindingRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/bindings$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, params, search) => ({
      body: await listCharacterBindings(service, params[0], search),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/bindings\/([^/]+)$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, params) => ({
      body: await getCharacterBinding(service, params[0], params[1]),
    }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/bindings\/([^/]+)$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, params, _search, body) => ({
      body: await patchCharacterBinding(
        service,
        params[0],
        params[1],
        wirePayload<UpdateCharacterBindingRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/bindings\/([^/]+)$/,
    tier: 'tier2',
    family: 'actors',
    status: 204,
    handle: async (service, params) => {
      await removeCharacterBinding(service, params[0], params[1]);
      return { body: null };
    },
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/actor-knowledge\/view$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, _params, _search, body) => ({
      body: await actorKnowledgeView(service, wirePayload<ViewRequest>(body, 'request')),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/actor-knowledge\/entries$/,
    tier: 'tier2',
    family: 'actors',
    status: 201,
    handle: async (service, _params, _search, body) => ({
      body: await addActorKnowledgeEntry(
        service,
        wirePayload<AddKnowledgeEntryRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/knowledge$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, params, search) => ({
      body: await listCharacterKnowledge(service, params[0], search),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/knowledge\/([^/]+)$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, params, search) => ({
      body: await getKnowledgeEntry(service, params[0], params[1]),
    }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/knowledge\/([^/]+)$/,
    tier: 'tier2',
    family: 'actors',
    handle: async (service, params, _search, body) => ({
      body: await patchKnowledgeEntry(
        service,
        params[0],
        params[1],
        wirePayload<UpdateKnowledgeEntryRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/characters\/([^/]+)\/knowledge\/([^/]+)$/,
    tier: 'tier2',
    family: 'actors',
    status: 204,
    handle: async (service, params, search) => {
      await deleteKnowledgeEntry(service, params[0], params[1], search);
      return { body: null };
    },
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/creators$/,
    tier: 'tier1',
    family: 'actors',
    handle: async (service, _params, search) => ({ body: await listCreators(service, search) }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/creators$/,
    tier: 'tier1',
    family: 'actors',
    status: 201,
    handle: async (service, _params, _search, body) => ({
      body: await createCreator(service, body),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/creators\/active$/,
    tier: 'tier1',
    family: 'actors',
    handle: async (service) => ({ body: await getActiveCreator(service) }),
  },
  {
    method: 'PUT',
    pattern: /^\/v1\/daemon\/creators\/active$/,
    tier: 'tier1',
    family: 'actors',
    handle: async (service, _params, _search, body) => ({
      body: await setActiveCreator(service, body),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/creators\/([^/]+)$/,
    tier: 'tier1',
    family: 'actors',
    handle: async (service, params) => ({ body: await getCreator(service, params[0]) }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/creators\/([^/]+)$/,
    tier: 'tier1',
    family: 'actors',
    handle: async (service, params, _search, body) => ({
      body: await patchCreator(service, params[0], body),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/creators\/([^/]+)$/,
    tier: 'tier1',
    family: 'actors',
    handle: async (service, params) => ({ body: await logoutCreator(service, params[0]) }),
  },
];
