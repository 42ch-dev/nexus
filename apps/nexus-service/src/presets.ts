/**
 * Preset + Strategy family HTTP surface (P5-T3): guarded preset authoring,
 * validation and Strategy canvas edits over the P3 `CoreService` authority
 * (`nexus_core::presets`). Thin translation only — the wire shapes are the
 * schema-owned generated DTOs, and every refusal (unknown preset, revision
 * conflict, validation failure) is the core's own.
 */
import type {
  ApprovedStrategySnapshot,
  CoreStrategyPatchResponse,
  GetPresetResponse,
  OrchestrationPresetListResponse,
  PresetProfileResponse,
  ReloadPresetResponse,
  ScaffoldPresetRequest,
  ScaffoldPresetResponse,
  SimulationTrajectory,
  StrategyApproveRequest,
  StrategyComposeRequest,
  StrategyComposeResponse,
  StrategyPatchPromptTemplateRequest,
  StrategyPatchStateRequest,
  StrategyPatchTransitionRequest,
  StrategySimulationRequest,
  UpdatePresetRequest,
  UpdatePresetResponse,
  ValidatePresetRequest,
  ValidatePresetResponse,
  WorkflowDocument,
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

/** `POST /v1/daemon/presets/{id}:reload` — re-resolve the manifest from disk. */
export function reloadPreset(
  service: ServiceCore,
  presetId: string,
): Promise<ReloadPresetResponse> {
  return withPrincipal(service, (principal) => service.core.reloadPreset(principal, presetId));
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

/**
 * `POST /v1/daemon/strategies/{strategy_id}/simulate`.
 *
 * Non-mutating: the body carries the whole candidate IR, so nothing is stored, no revision is
 * consumed and no Apply happens. Core validates the path id and the candidate state.
 */
export function simulateStrategy(
  service: ServiceCore,
  strategyId: string,
  request: StrategySimulationRequest,
): Promise<SimulationTrajectory> {
  return withPrincipal(service, (principal) =>
    service.core.simulateStrategy(principal, strategyId, request),
  );
}

/** `GET /v1/daemon/strategies/{strategy_id}/workflow-ir` — the installed strategy as candidate IR. */
export function strategyWorkflowIr(
  service: ServiceCore,
  strategyId: string,
): Promise<WorkflowDocument> {
  return withPrincipal(service, (principal) =>
    service.core.strategyWorkflowIr(principal, strategyId),
  );
}

/**
 * `GET /v1/daemon/strategies/{strategy_id}/candidate` — the canonical candidate.
 *
 * The stored compose row when one exists, otherwise the revision-`0` import projection the
 * `workflow-ir` read serves. The document's `revision` is the row revision, which is what a
 * stale compose reloads and re-applies against; the read never writes, so a strategy with no
 * row stays rowless.
 */
export function strategyCandidate(
  service: ServiceCore,
  strategyId: string,
): Promise<WorkflowDocument> {
  return withPrincipal(service, (principal) =>
    service.core.strategyCandidate(principal, strategyId),
  );
}

/**
 * `POST /v1/daemon/strategies/{strategy_id}/compose`.
 *
 * One typed operation per request, guarded by the `base_revision` the client last read. The
 * response carries the complete committed document, so the client replaces its candidate with
 * the server's copy rather than predicting normalization or resolved pins. A stale base is the
 * core's 409 `strategy_candidate_conflict`; a refused op is a 422 that names the nest or slot.
 */
export function composeStrategy(
  service: ServiceCore,
  strategyId: string,
  request: StrategyComposeRequest,
): Promise<StrategyComposeResponse> {
  return withPrincipal(service, (principal) =>
    service.core.composeStrategy(principal, strategyId, request),
  );
}

/**
 * `POST /v1/daemon/strategies/{strategy_id}/approve`.
 *
 * The write half of Approve: the core re-runs validation, pin verification and the
 * deterministic walk over the candidate at `base_revision`, requires the supplied trajectory to
 * be that walk's exact result, and freezes one immutable snapshot. A stale base is the core's
 * 409 `strategy_candidate_conflict`; a changed pin or missing/stale simulation is a 422
 * `invalid_input` whose `details` carry the refusal reason and hashes; a repeat approve of the
 * same content is the core's 409 `strategy_already_approved`.
 */
export function approveStrategy(
  service: ServiceCore,
  strategyId: string,
  request: StrategyApproveRequest,
): Promise<ApprovedStrategySnapshot> {
  return withPrincipal(service, (principal) =>
    service.core.approveStrategy(principal, strategyId, request),
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
    method: 'POST',
    pattern: /^\/v1\/daemon\/presets\/([^/]+):reload$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params) => ({ body: await reloadPreset(service, params[0]) }),
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
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/strategies\/([^/]+)\/workflow-ir$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params) => ({ body: await strategyWorkflowIr(service, params[0]) }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/strategies\/([^/]+)\/candidate$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params) => ({ body: await strategyCandidate(service, params[0]) }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/strategies\/([^/]+)\/compose$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params, _search, body) => ({
      body: await composeStrategy(
        service,
        params[0],
        wirePayload<StrategyComposeRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/strategies\/([^/]+)\/simulate$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params, _search, body) => ({
      body: await simulateStrategy(
        service,
        params[0],
        wirePayload<StrategySimulationRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/strategies\/([^/]+)\/approve$/,
    tier: 'tier2',
    family: 'presets',
    handle: async (service, params, _search, body) => ({
      body: await approveStrategy(
        service,
        params[0],
        wirePayload<StrategyApproveRequest>(body, 'request'),
      ),
    }),
  },
];
