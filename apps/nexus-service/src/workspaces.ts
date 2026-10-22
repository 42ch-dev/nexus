/**
 * Workspace family HTTP surface (plan v1219-shell-and-access T4): home-level
 * discovery, the active-selection read, and set-active over the frozen
 * `workspace` wire contracts (`schemas/daemon-api/workspace/…`).
 *
 * Selection is a home-control operation owned by the native entry; this
 * surface only translates transport, then refreshes the boot-time runtime
 * identity in place so `/v1/daemon/runtime/status` reports the new workspace
 * in the same session (first-run gate, access spec §3.1).
 *
 * Create is deliberately NOT mounted (compass D23 / issue I-000063): the
 * frozen `POST /v1/daemon/workspaces` identity keeps the retained 501
 * `route_not_migrated` refusal.
 */
import type {
  ActiveWorkspaceResponse,
  ListWorkspacesResponse,
  SetActiveWorkspaceRequest,
  SetActiveWorkspaceResponse,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { HttpError } from './errors.js';
import { parseOptionalInteger, refuseUnknownQueryKeys, wirePayload } from './world-kb.js';

/** Retained page default/cap applied to the core-discovered list. */
const DEFAULT_LIMIT = 50;
const MAX_LIMIT = 250;

type WorkspaceSummary = ListWorkspacesResponse['items'][number];

/** The retained opaque cursor: `<creator_id>/<workspace_slug>`. */
function cursorOf(item: WorkspaceSummary): string {
  return `${item.creator_id}/${item.workspace_slug}`;
}

/** `GET /v1/daemon/workspaces`. */
export async function listWorkspaces(
  service: ServiceCore,
  search: URLSearchParams,
): Promise<ListWorkspacesResponse> {
  refuseUnknownQueryKeys(search, ['creator_id', 'limit', 'cursor']);
  const creatorId = search.get('creator_id');
  const cursor = search.get('cursor');
  const limit =
    parseOptionalInteger(search, 'limit', { min: 1, max: MAX_LIMIT }) ?? DEFAULT_LIMIT;

  const discovered = await service.core.listWorkspaces();
  let items =
    creatorId === null
      ? discovered.items
      : discovered.items.filter((item) => item.creator_id === creatorId);
  if (cursor !== null) {
    const position = items.findIndex((item) => cursorOf(item) === cursor);
    // An unknown cursor must not silently re-serve page one: the caller would
    // page over data it already received (or over a changed list) believing it
    // continued. The retained validation style is a 400 `invalid_input`.
    if (position < 0) {
      throw new HttpError(400, 'invalid_input', `cursor '${cursor}' does not match any workspace`, {
        field: 'cursor',
      });
    }
    items = items.slice(position + 1);
  }

  const nextCursor = items.length > limit ? cursorOf(items[limit - 1]) : undefined;
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

/** `GET /v1/daemon/workspace` — retained 409 `uninitialized` before selection. */
export function getActiveWorkspace(service: ServiceCore): Promise<ActiveWorkspaceResponse> {
  return service.core.getActiveWorkspace();
}

/** `POST /v1/daemon/workspace/active`. */
export async function setActiveWorkspace(
  service: ServiceCore,
  body: unknown,
): Promise<SetActiveWorkspaceResponse> {
  const response = await service.core.selectWorkspace(
    wirePayload<SetActiveWorkspaceRequest>(body, 'request'),
  );
  service.creatorId = response.creator_id;
  service.workspaceSlug = response.workspace_slug;
  // A status-only shell publishes the full core in process once a workspace is
  // selected, so the first-run flow completes without a restart. Readiness is
  // claimed only after promotion (and, for a provider profile, the execution
  // owner) succeeded.
  if (!service.workspaceInitialized) await promoteShell(service);
  return response;
}

/**
 * Promote the status-only shell and publish the runtime readiness facts.
 *
 * The flag is written last: a failed promotion keeps `workspace_initialized`
 * false (the request fails and may be retried); the native side is idempotent
 * once the core is published, so a retry only re-attempts the missing step.
 */
async function promoteShell(service: ServiceCore): Promise<void> {
  await service.core.promoteCore();
  if (service.domainOnly) {
    // The domain-only profile owns no execution engine (openServiceCore's
    // same post-open facts).
    service.providerReady = true;
    service.engineEpoch = 0;
  } else {
    const owner = await service.core.startExecutionOwner();
    service.providerReady = owner.provider_ready;
    service.engineEpoch = owner.engine_epoch ?? 0;
  }
  service.workspaceInitialized = true;
}

/** Exact path/verb/tier identities this family owns (composer input). */
export const WORKSPACE_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/workspaces$/,
    tier: 'tier1',
    family: 'workspaces',
    handle: async (service, _params, search) => ({ body: await listWorkspaces(service, search) }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/workspace$/,
    tier: 'tier1',
    family: 'workspaces',
    handle: async (service) => ({ body: await getActiveWorkspace(service) }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/workspace\/active$/,
    tier: 'tier1',
    family: 'workspaces',
    handle: async (service, _params, _search, body) => ({
      body: await setActiveWorkspace(service, body),
    }),
  },
];
