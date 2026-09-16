import type {
  CoreChangesRequest,
  CoreHostQuery,
  WorldKbKeyBlockStateResponse,
  WorldKbPatchEntityRequest,
  WorldKbPatchRelationshipRequest,
  WorldKbPatchRelationshipResponse,
  WorldKbPromoteCandidateRequest,
  WorldKbPromoteCandidateResponse,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import { HttpError, mapNativeError } from './errors.js';

/**
 * Shared domain-HTTP plumbing for the P5-T1 World / Work / content /
 * knowledge family modules: the single Principal seam (the handle is minted
 * natively and only proves the stored identity), the retained World KB
 * family operations, and the query-grammar helpers the route composer calls
 * before dispatch. Family business lives in the family modules.
 */

export async function withPrincipal<T>(
  service: ServiceCore,
  fn: (principal: Awaited<ReturnType<ServiceCore['core']['activePrincipal']>>) => Promise<T>,
): Promise<T> {
  try {
    const principal = await service.core.activePrincipal();
    return await fn(principal);
  } catch (error) {
    throw mapNativeError(error);
  }
}

export async function getWorldKbGraph(
  service: ServiceCore,
  worldId: string,
  includeSuggested: boolean,
) {
  return withPrincipal(service, async (principal) =>
    service.core.worldKbGraph(principal, worldId, includeSuggested),
  );
}

export async function patchWorldKbEntity(
  service: ServiceCore,
  worldId: string,
  request: WorldKbPatchEntityRequest,
) {
  return withPrincipal(service, async (principal) =>
    service.core.patchWorldKbEntity(principal, worldId, request),
  );
}

export async function getWorldKbCandidates(
  service: ServiceCore,
  worldId: string,
  limit?: number,
  cursor?: string,
) {
  return withPrincipal(service, async (principal) =>
    service.core.worldKbCandidates(principal, worldId, limit, cursor),
  );
}

/** `POST /v1/daemon/worlds/{world_id}/kb/promote-candidate`. */
export async function promoteWorldKbCandidate(
  service: ServiceCore,
  worldId: string,
  request: WorldKbPromoteCandidateRequest,
): Promise<WorldKbPromoteCandidateResponse> {
  return withPrincipal(service, async (principal) =>
    service.core.promoteWorldKbCandidate(principal, worldId, request),
  );
}

/** `POST /v1/daemon/worlds/{world_id}/kb/patch-relationship`. */
export async function patchWorldKbRelationship(
  service: ServiceCore,
  worldId: string,
  request: WorldKbPatchRelationshipRequest,
): Promise<WorldKbPatchRelationshipResponse> {
  return withPrincipal(service, async (principal) =>
    service.core.patchWorldKbRelationship(principal, worldId, request),
  );
}

/** `GET /v1/daemon/worlds/{world_id}/kb/key-blocks/{key_block_id}/state`. */
export async function getWorldKbKeyBlockState(
  service: ServiceCore,
  worldId: string,
  keyBlockId: string,
): Promise<WorldKbKeyBlockStateResponse> {
  return withPrincipal(service, async (principal) =>
    service.core.worldKbKeyBlockState(principal, worldId, keyBlockId),
  );
}

export async function getCoreChanges(service: ServiceCore, request: CoreChangesRequest) {
  return withPrincipal(service, async (principal) => service.core.changes(principal, request));
}

export async function hostQuery(service: ServiceCore, request: CoreHostQuery) {
  try {
    return await service.core.hostQuery(request);
  } catch (error) {
    throw mapNativeError(error);
  }
}

export function parseIncludeSuggested(searchParams: URLSearchParams): boolean {
  const raw = searchParams.get('include_suggested');
  if (raw === null) return false;
  if (raw === 'true' || raw === '1') return true;
  if (raw === 'false' || raw === '0') return false;
  throw new HttpError(400, 'invalid_input', 'include_suggested must be a boolean');
}

function parseStrictIntegerToken(value: string, field: string): number {
  if (!/^-?\d+$/.test(value)) {
    throw new HttpError(400, 'invalid_input', `${field} must be a decimal integer`);
  }
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed)) {
    throw new HttpError(400, 'invalid_input', `${field} must be a safe integer`);
  }
  return parsed;
}

/** Candidate/session pagination: absent -> default, then clamp to 1..max. */
export function parseClampedLimit(
  value: string | null,
  field: string,
  { defaultLimit = 50, max = 250 }: { defaultLimit?: number; max?: number } = {},
): number {
  if (value === null) return defaultLimit;
  const parsed = parseStrictIntegerToken(value, field);
  return Math.min(Math.max(parsed, 1), max);
}

/** Core changes limit: absent -> undefined (native default), otherwise strict 1..max. */
export function parseBoundedLimit(
  value: string | null,
  field: string,
  { min = 1, max = 256 }: { min?: number; max?: number } = {},
): number | undefined {
  if (value === null) return undefined;
  const parsed = parseStrictIntegerToken(value, field);
  if (parsed < min || parsed > max) {
    throw new HttpError(400, 'invalid_input', `${field} must be an integer between ${min} and ${max}`);
  }
  return parsed;
}

/** Optional boolean query parameter: absent -> `undefined`, strict otherwise. */
export function parseOptionalBoolean(
  searchParams: URLSearchParams,
  field: string,
): boolean | undefined {
  const raw = searchParams.get(field);
  if (raw === null) return undefined;
  if (raw === 'true' || raw === '1') return true;
  if (raw === 'false' || raw === '0') return false;
  throw new HttpError(400, 'invalid_input', `${field} must be a boolean`);
}

/** Optional integer query parameter: absent -> `undefined`, strict otherwise. */
export function parseOptionalInteger(
  searchParams: URLSearchParams,
  field: string,
  { min = 0, max = 4_294_967_295 }: { min?: number; max?: number } = {},
): number | undefined {
  const raw = searchParams.get(field);
  if (raw === null) return undefined;
  const parsed = parseStrictIntegerToken(raw, field);
  if (parsed < min || parsed > max) {
    throw new HttpError(
      400,
      'invalid_input',
      `${field} must be an integer between ${min} and ${max}`,
    );
  }
  return parsed;
}

/**
 * A family handler body/query payload is an already-`JSON.parse`d value; the
 * native layer re-parses it into the generated DTO, so this cast only carries
 * the transport contract (the schema stays the shape authority).
 */
export function wirePayload<T>(body: unknown, label: string): T {
  if (body === undefined || body === null || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'invalid_input', `${label} must be a JSON object`);
  }
  return body as T;
}
