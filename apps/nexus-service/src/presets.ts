/**
 * Preset + Strategy family HTTP surface (P5-T3): guarded preset authoring,
 * validation and Strategy canvas edits over the P3 `CoreService` authority
 * (`nexus_core::presets`). Thin translation only — the wire shapes are the
 * schema-owned generated DTOs, and every refusal (unknown preset, revision
 * conflict, validation failure) is the core's own.
 */
import type {
  CoreStrategyPatchResponse,
  GetPresetResponse,
  OrchestrationPresetListResponse,
  PresetProfileResponse,
  ScaffoldPresetRequest,
  ScaffoldPresetResponse,
  StrategyPatchPromptTemplateRequest,
  StrategyPatchStateRequest,
  StrategyPatchTransitionRequest,
  UpdatePresetRequest,
  UpdatePresetResponse,
  ValidatePresetRequest,
  ValidatePresetResponse,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { withPrincipal, wirePayload } from './world-kb.js';

/** `GET /v1/daemon/presets`. */
export function listPresets(service: ServiceCore) {
  return withPrincipal(service, (principal) => service.core.listPresets(principal));
}

/** `GET /v1/daemon/presets/{id}`. */
export function getPreset(service: ServiceCore, presetId: string): Promise<GetPresetResponse> {
  return withPrincipal(service, (principal) => service.core.getPreset(principal, presetId));
}

/** `POST /v1/daemon/presets` — scaffold, 201 created. */
export function scaffoldPreset(
  service: ServiceCore,
  request: ScaffoldPresetRequest,
): Promise<ScaffoldPresetResponse> {
  return withPrincipal(service, (principal) => service.core.scaffoldPreset(principal, request));
}

/** `POST /v1/daemon/presets:validate`. */
export function validatePreset(
  service: ServiceCore,
  request: ValidatePresetRequest,
): Promise<ValidatePresetResponse> {
  return withPrincipal(service, (principal) => service.core.validatePreset(principal, request));
}

/** `PATCH /v1/daemon/presets/{id}`. */
export function updatePreset(
  service: ServiceCore,
  presetId: string,
  request: UpdatePresetRequest,
): Promise<UpdatePresetResponse> {
  return withPrincipal(service, (principal) =>
    service.core.updatePreset(principal, presetId, request),
  );
}

/** `DELETE /v1/daemon/presets/{id}` — 204. */
export async function deletePreset(service: ServiceCore, presetId: string): Promise<void> {
  await withPrincipal(service, (principal) => service.core.deletePreset(principal, presetId));
}

/** `GET /v1/daemon/orchestration/presets`. */
export function listOrchestrationPresets(
  service: ServiceCore,
): Promise<OrchestrationPresetListResponse> {
  return withPrincipal(service, (principal) =>
    service.core.listOrchestrationPresets(principal),
  );
}

/** `GET /v1/daemon/orchestration/presets/{id}/profile`. */
export function getPresetProfile(
  service: ServiceCore,
  presetId: string,
): Promise<PresetProfileResponse> {
  return withPrincipal(service, (principal) =>
    service.core.getPresetProfile(principal, presetId),
  );
}

/** `POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/patch`. */
export function patchStrategyState(
  service: ServiceCore,
  strategyId: string,
  stateId: string,
  request: StrategyPatchStateRequest,
): Promise<CoreStrategyPatchResponse> {
  return withPrincipal(service, (principal) =>
    service.core.patchStrategyState(principal, strategyId, stateId, request),
  );
}

/** `POST /v1/daemon/strategies/{strategy_id}/transitions/patch`. */
export function patchStrategyTransition(
  service: ServiceCore,
  strategyId: string,
  request: StrategyPatchTransitionRequest,
): Promise<CoreStrategyPatchResponse> {
  return withPrincipal(service, (principal) =>
    service.core.patchStrategyTransition(principal, strategyId, request),
  );
}

/** `POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/prompt/patch`. */
export function patchStrategyPromptTemplate(
  service: ServiceCore,
  strategyId: string,
  stateId: string,
  request: StrategyPatchPromptTemplateRequest,
): Promise<CoreStrategyPatchResponse> {
  return withPrincipal(service, (principal) =>
    service.core.patchStrategyPromptTemplate(principal, strategyId, stateId, request),
  );
}

/** Exact path/verb/tier identities this family owns (composer input). */
export const PRESET_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/presets$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service) => ({ body: await listPresets(service) }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/presets$/,
    tier: 'tier2',
    family: 'presets',
    status: 201,
    handle: async (service, _params, _search, body) => ({
      body: await scaffoldPreset(service, wirePayload<ScaffoldPresetRequest>(body, 'request')),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/presets:validate$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, _params, _search, body) => ({
      body: await validatePreset(service, wirePayload<ValidatePresetRequest>(body, 'request')),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/presets\/([^/]+)$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params) => ({ body: await getPreset(service, params[0]) }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/presets\/([^/]+)$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params, _search, body) => ({
      body: await updatePreset(service, params[0], wirePayload<UpdatePresetRequest>(body, 'request')),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/presets\/([^/]+)$/,
    tier: 'tier2',
    family: 'presets',
    status: 204,
    handle: async (service, params) => {
      await deletePreset(service, params[0]);
      return { body: null };
    },
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/orchestration\/presets$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service) => ({ body: await listOrchestrationPresets(service) }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/orchestration\/presets\/([^/]+)\/profile$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params) => ({ body: await getPresetProfile(service, params[0]) }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/strategies\/([^/]+)\/states\/([^/]+)\/patch$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params, _search, body) => ({
      body: await patchStrategyState(
        service,
        params[0],
        params[1],
        wirePayload<StrategyPatchStateRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/strategies\/([^/]+)\/transitions\/patch$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params, _search, body) => ({
      body: await patchStrategyTransition(
        service,
        params[0],
        wirePayload<StrategyPatchTransitionRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/strategies\/([^/]+)\/states\/([^/]+)\/prompt\/patch$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params, _search, body) => ({
      body: await patchStrategyPromptTemplate(
        service,
        params[0],
        params[1],
        wirePayload<StrategyPatchPromptTemplateRequest>(body, 'request'),
      ),
    }),
  },
];
