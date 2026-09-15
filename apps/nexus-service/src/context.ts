/**
 * Context family HTTP surface (P5-T2): the moment inspector (read-only
 * assembly observation) and the moment directive (thin set/show/clear
 * wrapper) over the P2-T2 context authority. Every operation is a thin
 * translation over the native family surface; ownership of the observed
 * World and directive scopes stays in the single Rust core authority.
 */
import type {
  MomentDirectiveRequest,
  MomentDirectiveResponse,
  MomentInspectRequest,
  MomentInspectResponse,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { withPrincipal, wirePayload } from './world-kb.js';

/** `POST /v1/daemon/inspector/moment` — observational read-only assembly. */
export function inspectMoment(
  service: ServiceCore,
  request: MomentInspectRequest,
): Promise<MomentInspectResponse> {
  return withPrincipal(service, (principal) => service.core.inspectMoment(principal, request));
}

/** `POST /v1/daemon/moment-directive`. */
export function momentDirective(
  service: ServiceCore,
  request: MomentDirectiveRequest,
): Promise<MomentDirectiveResponse> {
  return withPrincipal(service, (principal) => service.core.momentDirective(principal, request));
}

/** Exact path/verb/tier identities this family owns (composer input). */
export const CONTEXT_ROUTES: readonly DomainRoute[] = [
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/inspector\/moment$/,
    tier: 'tier2',
    family: 'context',
    handle: async (service, _params, _search, body) => ({
      body: await inspectMoment(service, wirePayload<MomentInspectRequest>(body, 'request')),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/moment-directive$/,
    tier: 'tier2',
    family: 'context',
    handle: async (service, _params, _search, body) => ({
      body: await momentDirective(service, wirePayload<MomentDirectiveRequest>(body, 'request')),
    }),
  },
];
