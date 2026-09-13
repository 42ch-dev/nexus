import type {
  CoreChangesRequest,
  CoreHostQuery,
  WorldKbPatchEntityRequest,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import { HttpError, mapNativeError } from './errors.js';

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

export function parsePositiveInt(
  value: string | null,
  field: string,
  { min = 1, max = 250 }: { min?: number; max?: number } = {},
): number | undefined {
  if (value === null) return undefined;
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed < min || parsed > max) {
    throw new HttpError(400, 'invalid_input', `${field} must be an integer between ${min} and ${max}`);
  }
  return parsed;
}
