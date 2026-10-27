/**
 * Capabilities family (CAP-001 / I-000125): the live orchestration capability
 * registry.
 *
 * The wire shape is the hand-coded local DTO (`camelCase` schemas, origin) the
 * web capability browser already mirrors in
 * `apps/web/src/lib/nexus/capability-info.ts`; the generated
 * `ListCapabilitiesResponse` is schema-facing (snake_case) and is not what this
 * tier serves. Pagination/sort live here so the native lane stays a passthrough.
 */
import { HttpError } from './errors.js';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { parseClampedLimit } from './world-kb.js';

interface CapabilityRow {
  name: string;
  inputSchema: string;
  outputSchema: string;
  origin: 'builtin' | 'user';
}

export async function listCapabilities(service: ServiceCore, search: URLSearchParams) {
  const limit = parseClampedLimit(search.get('limit'), 'limit');
  const sort = search.get('sort') ?? 'name';
  if (sort !== 'name' && sort !== '-name') {
    throw new HttpError(400, 'invalid_input', 'sort must be "name" or "-name"', {
      field: 'sort',
    });
  }
  const raw = (await service.core.listCapabilities()) as { items: CapabilityRow[] };
  let items = [...raw.items].sort((left, right) => left.name.localeCompare(right.name));
  if (sort === '-name') items.reverse();

  const cursor = search.get('cursor');
  if (cursor !== null) {
    const position = items.findIndex((item) => item.name === cursor);
    // An unknown cursor must not silently re-serve page one.
    if (position < 0) {
      throw new HttpError(400, 'invalid_input', `cursor '${cursor}' does not match any capability`, {
        field: 'cursor',
      });
    }
    items = items.slice(position + 1);
  }

  const nextCursor = items.length > limit ? items[limit - 1]!.name : undefined;
  if (nextCursor !== undefined) items = items.slice(0, limit);
  return {
    items,
    pagination: {
      limit,
      has_more: nextCursor !== undefined,
      ...(nextCursor !== undefined ? { next_cursor: nextCursor } : {}),
    },
  };
}

export const CAPABILITY_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/orchestration\/capabilities$/,
    tier: 'tier2',
    family: 'execution',
    handle: async (service, _params, search) => {
      // A domain-only profile owns no provider lane: the capability registry
      // is outside that profile, not an unmigrated route.
      if (service.domainOnly) {
        throw new HttpError(
          501,
          'not_supported',
          'capabilities require a provider-enabled profile',
        );
      }
      return { body: await listCapabilities(service, search) };
    },
  },
];
