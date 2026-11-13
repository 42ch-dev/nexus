/**
 * Pattern package registry family (v1.239 P3-T4): the read-only listing of the
 * local packages under `<home>/.nexus42/patterns`.
 *
 * The reader owns discovery, manifest validation and the fragment pin; nothing
 * here writes, creates the directory or loads a capability. Pagination lives in
 * this tier so the native lane stays a passthrough: the cursor is the
 * `package_dir` of the last served item, and a cursor that names no package is a
 * typed 400 rather than a silent re-serve of page one.
 */
import type {
  ListPatternsQuery,
  ListPatternsResponse,
  PatternSummary,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { HttpError } from './errors.js';
import { parseOptionalInteger, refuseUnknownQueryKeys, withPrincipal } from './world-kb.js';

/** Page size when the query names none. */
const DEFAULT_LIMIT = 100;

/** `GET /v1/daemon/patterns` — cursor-paginated, `package_dir` ascending. */
export async function listPatterns(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<ListPatternsResponse> {
  refuseUnknownQueryKeys(searchParams, ['cursor', 'limit']);
  const limit = parseOptionalInteger(searchParams, 'limit', { min: 1 }) ?? DEFAULT_LIMIT;
  const cursor = searchParams.get('cursor');
  const query: ListPatternsQuery = {
    ...(cursor !== null ? { cursor } : {}),
    limit,
  };

  const scanned = await withPrincipal(service, (principal) => service.core.listPatterns(principal));
  // The listing contract owns the order, not the scan that happened to produce it.
  let items: PatternSummary[] = [...scanned].sort((left, right) =>
    left.package_dir.localeCompare(right.package_dir),
  );
  if (query.cursor !== undefined) {
    const position = items.findIndex((item) => item.package_dir === query.cursor);
    if (position < 0) {
      throw new HttpError(400, 'invalid_input', `cursor '${query.cursor}' does not match any package`, {
        field: 'cursor',
      });
    }
    items = items.slice(position + 1);
  }

  const nextCursor = items.length > limit ? items[limit - 1]!.package_dir : undefined;
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

/** Exact path/verb/tier identities this family owns (composer input). */
export const PATTERN_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/patterns$/,
    tier: 'tier2',
    family: 'patterns',
    handle: async (service, _params, search) => ({ body: await listPatterns(service, search) }),
  },
];
