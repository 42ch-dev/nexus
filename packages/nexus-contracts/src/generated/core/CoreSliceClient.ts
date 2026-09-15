/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/core/core-service-operations.schema.json
 * Generator: tooling/codegen/src/client-gen.ts
 */

import type { WorldKbGraphResponse, WorldKbPatchEntityRequest, WorldKbPatchEntityResponse, WorldKbCandidatesResponse, WorldKbPromoteCandidateRequest, WorldKbPromoteCandidateResponse, WorldKbPatchRelationshipRequest, WorldKbPatchRelationshipResponse, WorldKbKeyBlockStateResponse, CoreChangesRequest, CoreChangesResponse, CreateSessionRequest, AgentHostListSessionsQuery, SessionListResponse, SessionResponse, ShutdownSessionResponse, ExecuteOperationRequest, OperationResponse, CancelOperationResponse, CoreServiceStopRequest, RuntimeApi, ProviderHostEvent, NarrativeWorldsListResponse, NarrativeWorldResponse, CreateWorldRequest, CreateWorldResponse, CreateForkRequest, CreateForkResponse, PackExportRequest, PackExportResponse, PackImportRequest, PackImportResponse, WorldRulesListResponse, WorldRuleCreateRequest, WorldRuleResponse, WorldRuleUpdateRequest, WorldFindingsListResponse, TimelineOverviewResponse, CoreTimelineEventsQuery, ListTimelineEventsResponse, ListWorksQuery, ListWorksResponse, CreateWorkRequest, CreateWorkResponse, WorkDetailResponse, PatchWorkRequest, WorkPoolListQuery, WorkPoolListResponse, WorkPoolSetActiveRequest, WorkPoolEntry, WorkPoolPromoteRequest, WorkPoolArchiveRequest, WorkInspirationListQuery, WorkInspirationListResponse, WorkInspirationAddRequest, WorkInspirationAddResponse, WorkInspirationPromoteRequest, WorkInspirationPromoteResponse, WorkInspirationArchiveRequest, WorkInspirationItem, AppendInspirationRequest, AppendInspirationResponse, ReleaseCompletionLockRequest, WorkReconcileReport, ListChaptersQuery, ListChaptersResponse, ChapterContentQuery, ChapterDetail, ChapterOutline, ChapterBody, PatchChapterRequest, WorkOutline, OutlinePatchStructureRequest, OutlinePatchResponse, OutlinePatchChapterRequest, TimelinePatchEventRequest, ListKbEntriesQuery, ListKbEntriesResponse, AddKbEntryRequest, AddKbEntryResponse, GetKbEntryResponse, DeleteKbEntryResponse, ListFindingsQuery, ListFindingsResponse, CreateFindingRequest, FindingDetailResponse, UpdateFindingRequest, BatchUpdateFindingsRequest, BatchUpdateFindingsResponse, StaleFindingsResponse, FindingsPruneResponse, ReadingProgressRequest, ReadingProgressResponse, ReadingAnnotationCreateRequest, ReadingAnnotation, ReadingAnnotationPatchRequest, ReadingAnnotationListResponse, ReferenceListResponse, ReferenceGetResponse } from '../index';
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
  listNarrativeWorlds(): Promise<NarrativeWorldsListResponse>;
  getWorld(worldId: string): Promise<NarrativeWorldResponse>;
  createWorld(request: CreateWorldRequest): Promise<CreateWorldResponse>;
  deleteWorld(worldId: string): Promise<void>;
  createFork(worldId: string, request: CreateForkRequest): Promise<CreateForkResponse>;
  exportPack(worldId: string, request?: PackExportRequest): Promise<PackExportResponse>;
  importPack(worldId: string, request: PackImportRequest): Promise<PackImportResponse>;
  listWorldRules(worldId: string): Promise<WorldRulesListResponse>;
  createWorldRule(worldId: string, body: WorldRuleCreateRequest): Promise<WorldRuleResponse>;
  updateWorldRule(worldId: string, ruleId: string, body: WorldRuleUpdateRequest): Promise<WorldRuleResponse>;
  listWorldFindings(worldId: string): Promise<WorldFindingsListResponse>;
  getTimelineOverview(cursor?: string): Promise<TimelineOverviewResponse>;
  getTimelineEvents(worldId: string, query?: CoreTimelineEventsQuery): Promise<ListTimelineEventsResponse>;
  listWorks(query?: ListWorksQuery): Promise<ListWorksResponse>;
  createWork(request: CreateWorkRequest): Promise<CreateWorkResponse>;
  getWork(workId: string): Promise<WorkDetailResponse>;
  patchWork(workId: string, request: PatchWorkRequest): Promise<WorkDetailResponse>;
  deleteWork(workId: string): Promise<void>;
  listWorkPool(query?: WorkPoolListQuery): Promise<WorkPoolListResponse>;
  setWorkPoolActive(request: WorkPoolSetActiveRequest): Promise<WorkPoolEntry>;
  promoteWorkPoolEntry(request: WorkPoolPromoteRequest): Promise<WorkPoolEntry>;
  archiveWorkPoolEntry(request: WorkPoolArchiveRequest): Promise<WorkPoolEntry>;
  listWorkInspiration(query?: WorkInspirationListQuery): Promise<WorkInspirationListResponse>;
  addWorkInspiration(request: WorkInspirationAddRequest): Promise<WorkInspirationAddResponse>;
  promoteWorkInspiration(request: WorkInspirationPromoteRequest): Promise<WorkInspirationPromoteResponse>;
  archiveWorkInspiration(request: WorkInspirationArchiveRequest): Promise<WorkInspirationItem>;
  appendWorkInspiration(workId: string, request: AppendInspirationRequest): Promise<AppendInspirationResponse>;
  releaseWorkCompletionLock(workId: string, request: ReleaseCompletionLockRequest): Promise<WorkDetailResponse>;
  reconcileWorkChapters(workId: string, query?: { dry_run?: boolean }): Promise<WorkReconcileReport>;
  listChapters(workId: string, query?: ListChaptersQuery): Promise<ListChaptersResponse>;
  getChapter(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterDetail>;
  getChapterOutline(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterOutline>;
  getChapterBody(workId: string, chapter: number, query?: ChapterContentQuery): Promise<ChapterBody>;
  patchChapter(workId: string, chapter: number, request: PatchChapterRequest, query?: ChapterContentQuery): Promise<ChapterDetail>;
  getWorkOutline(workId: string): Promise<WorkOutline>;
  patchOutlineStructure(workId: string, request: OutlinePatchStructureRequest): Promise<OutlinePatchResponse>;
  patchOutlineChapter(workId: string, chapter: number, request: OutlinePatchChapterRequest): Promise<OutlinePatchResponse>;
  patchTimelineEvent(workId: string, request: TimelinePatchEventRequest): Promise<OutlinePatchResponse>;
  listKbEntries(query?: ListKbEntriesQuery): Promise<ListKbEntriesResponse>;
  addKbEntry(request: AddKbEntryRequest): Promise<AddKbEntryResponse>;
  getKbEntry(entryId: string): Promise<GetKbEntryResponse>;
  deleteKbEntry(entryId: string): Promise<DeleteKbEntryResponse>;
  listFindings(workId: string, query?: ListFindingsQuery): Promise<ListFindingsResponse>;
  createFinding(workId: string, request: CreateFindingRequest): Promise<FindingDetailResponse>;
  createFindingFromReview(workId: string, request: CreateFindingRequest): Promise<FindingDetailResponse>;
  getWorkFinding(workId: string, findingId: string): Promise<FindingDetailResponse>;
  updateFinding(workId: string, findingId: string, patch: UpdateFindingRequest): Promise<FindingDetailResponse>;
  deleteFinding(workId: string, findingId: string): Promise<void>;
  listStaleFindings(): Promise<StaleFindingsResponse>;
  batchUpdateFindings(request: BatchUpdateFindingsRequest): Promise<BatchUpdateFindingsResponse>;
  pruneFindings(query?: { older_than_days?: number; dry_run?: boolean }): Promise<FindingsPruneResponse>;
  getFinding(findingId: string): Promise<FindingDetailResponse>;
  getReadingProgress(workId: string, chapter: number): Promise<ReadingProgressResponse>;
  putReadingProgress(request: ReadingProgressRequest): Promise<ReadingProgressResponse>;
  deleteReadingProgress(workId: string, chapter: number): Promise<void>;
  listReadingAnnotations(workId: string, chapter: number): Promise<ReadingAnnotationListResponse>;
  createReadingAnnotation(request: ReadingAnnotationCreateRequest): Promise<ReadingAnnotation>;
  patchReadingAnnotation(annotationId: string, request: ReadingAnnotationPatchRequest): Promise<ReadingAnnotation>;
  deleteReadingAnnotation(annotationId: string): Promise<void>;
  listReferences(): Promise<ReferenceListResponse>;
  getReference(referenceId: string): Promise<ReferenceGetResponse>;
  worldKbPromoteCandidate(worldId: string, request: WorldKbPromoteCandidateRequest): Promise<WorldKbPromoteCandidateResponse>;
  worldKbPatchRelationship(worldId: string, request: WorldKbPatchRelationshipRequest): Promise<WorldKbPatchRelationshipResponse>;
  getKeyBlockState(worldId: string, keyBlockId: string): Promise<WorldKbKeyBlockStateResponse>;
}
