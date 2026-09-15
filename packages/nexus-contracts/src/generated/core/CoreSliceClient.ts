/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/core/core-service-operations.schema.json
 * Generator: tooling/codegen/src/client-gen.ts
 */

import type { WorldKbGraphResponse, WorldKbPatchEntityRequest, WorldKbPatchEntityResponse, WorldKbCandidatesResponse, CoreChangesRequest, CoreChangesResponse, CreateSessionRequest, AgentHostListSessionsQuery, SessionListResponse, SessionResponse, ShutdownSessionResponse, ExecuteOperationRequest, OperationResponse, CancelOperationResponse, CoreServiceStopRequest, RuntimeApi, ProviderHostEvent } from '../index';
import type { CoreStreamGap } from './provider-event-batch';

export interface CoreSliceClient {
  getWorldKbGraph(worldId: string, query?: { includeSuggested?: boolean }): Promise<WorldKbGraphResponse>;
  worldKbPatchEntity(worldId: string, request: WorldKbPatchEntityRequest): Promise<WorldKbPatchEntityResponse>;
  getWorldKbCandidates(worldId: string, query?: { limit?: number; cursor?: string }): Promise<WorldKbCandidatesResponse>;
  getCoreChanges(request: CoreChangesRequest): Promise<CoreChangesResponse>;
  createAgentHostSession(request: CreateSessionRequest): Promise<SessionResponse>;
  listAgentHostSessions(query?: AgentHostListSessionsQuery): Promise<SessionListResponse>;
  getAgentHostSession(sessionId: string): Promise<SessionResponse>;
  shutdownAgentHostSession(sessionId: string): Promise<ShutdownSessionResponse>;
  executeAgentHostOperation(sessionId: string, request: ExecuteOperationRequest): Promise<OperationResponse>;
  getAgentHostOperation(operationId: string): Promise<OperationResponse>;
  cancelAgentHostOperation(operationId: string): Promise<CancelOperationResponse>;
  stopService(request: CoreServiceStopRequest): Promise<RuntimeApi>;
  subscribeAgentHostEvents(sessionId: string, signal: AbortSignal): AsyncIterable<ProviderHostEvent | CoreStreamGap>;
}
