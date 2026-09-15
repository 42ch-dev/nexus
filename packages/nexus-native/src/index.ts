import type {
  CoreChangesRequest,
  CoreChangesResponse,
  CoreCloseReport,
  CoreHostQuery,
  CoreHostQueryResponse,
  NativeCompatibility,
  NativeOpenOptions,
  ProviderCall,
  ProviderEventBatch,
  ProviderReply,
  WorldKbCandidatesResponse,
  WorldKbGraphResponse,
  WorkDetailResponse,
  WorkInspirationAddRequest,
  WorkInspirationAddResponse,
  WorkInspirationArchiveRequest,
  WorkInspirationItem,
  WorkInspirationListQuery,
  WorkInspirationListResponse,
  WorkInspirationPromoteRequest,
  WorkInspirationPromoteResponse,
  WorkPoolArchiveRequest,
  WorkPoolEntry,
  WorkPoolListQuery,
  WorkPoolListResponse,
  WorkPoolPromoteRequest,
  WorkPoolSetActiveRequest,
  WorkReconcileReport,
  AppendInspirationRequest,
  AppendInspirationResponse,
  ReleaseCompletionLockRequest,
  WorldKbPatchEntityRequest,
  WorldKbPatchEntityResponse,
  WorldKbKeyBlockStateResponse,
  WorldKbPatchRelationshipRequest,
  WorldKbPatchRelationshipResponse,
  WorldKbPromoteCandidateRequest,
  WorldKbPromoteCandidateResponse,
  CreateWorldRequest,
  CreateWorldResponse,
  CreateWorkRequest,
  CreateWorkResponse,
  CreateForkRequest,
  CreateForkResponse,
  PackExportRequest,
  PackExportResponse,
  PackImportRequest,
  PackImportResponse,
  WorldFindingsListResponse,
  WorldRuleCreateRequest,
  WorldRuleResponse,
  WorldRuleUpdateRequest,
  WorldRulesListResponse,
  CoreTimelineOverviewQuery,
  CoreTimelineEventsQuery,
  TimelineOverviewResponse,
  CoreWorkSelection,
  CoreChapterContentQuery,
  ListChaptersQuery,
  ListChaptersResponse,
  ListWorksQuery,
  ListWorksResponse,
  PatchWorkRequest,
  ListTimelineEventsResponse,
  ChapterDetail,
  ChapterOutline,
  ChapterBody,
  PatchChapterRequest,
  WorkOutline,
  OutlinePatchResponse,
  OutlinePatchStructureRequest,
  OutlinePatchChapterRequest,
  TimelinePatchEventRequest,
  ListKbEntriesQuery,
  ListKbEntriesResponse,
  AddKbEntryRequest,
  AddKbEntryResponse,
  GetKbEntryResponse,
  DeleteKbEntryResponse,
  CreateFindingRequest,
  FindingDetailResponse,
  ListFindingsQuery,
  ListFindingsResponse,
  UpdateFindingRequest,
  BatchUpdateFindingsRequest,
  BatchUpdateFindingsResponse,
  StaleFindingsResponse,
  FindingsPruneResponse,
  ReadingProgressQuery,
  ReadingProgressRequest,
  ReadingProgressResponse,
  ReadingAnnotationListQuery,
  ReadingAnnotationListResponse,
  ReadingAnnotationCreateRequest,
  ReadingAnnotation,
  ReadingAnnotationPatchRequest,
  ReferenceListResponse,
  ReferenceGetResponse,
  NarrativeWorldsListResponse,
  NarrativeWorldResponse,
  ListCharactersQuery,
  ListCharactersResponse,
  CreateCharacterRequest,
  CreateCharacterResponse,
  CharacterDetail,
  UpdateCharacterRequest,
  CharacterLifecycleRequest,
  AddCharacterBindingRequest,
  AddCharacterBindingResponse,
  ListCharacterBindingsQuery,
  ListCharacterBindingsResponse,
  CharacterBindingDetail,
  UpdateCharacterBindingRequest,
  ViewRequest,
  ViewResponse,
  AddKnowledgeEntryRequest,
  AddKnowledgeEntryResponse,
  ListCharacterKnowledgeQuery,
  ListCharacterKnowledgeResponse,
  KnowledgeEntryDetail,
  UpdateKnowledgeEntryRequest,
  ListCreatorsQuery,
  ListCreatorsResponse,
  CreatorDetail,
  SetActiveCreatorRequest,
  SetActiveCreatorResponse,
  ActiveCreatorResponse,
  LogoutResponse,
  CaptureCharacterPendingReviewRequest,
  CaptureCharacterPendingReviewResponse,
  ListCharacterPendingReviewsQuery,
  ListCharacterPendingReviewsResponse,
  CountCharacterPendingReviewsQuery,
  CountCharacterPendingReviewsResponse,
  DeleteCharacterPendingReviewResponse,
  ReviewCharacterMemoryRequest,
  ReviewCharacterMemoryResponse,
  ListCharacterMemoryFragmentsQuery,
  ListCharacterMemoryFragmentsResponse,
  PromoteCharacterFragmentRequest,
  PromoteCharacterFragmentResponse,
  CharacterSoulNarrativeRequest,
  CharacterSoulNarrativeResponse,
  RecordCharacterTomRequest,
  RecordCharacterTomResponse,
  ListCharacterTomQuery,
  ListCharacterTomResponse,
  ListPendingReviewsQuery,
  ListPendingReviewsResponse,
  CountPendingReviewsResponse,
  DeletePendingReviewResponse,
  ReviewRequest,
  ReviewResponse,
  ListMemoryFragmentsQuery,
  ListMemoryFragmentsResponse,
  SoulNarrativeRequest,
  SoulNarrativeResponse,
  MomentInspectRequest,
  MomentInspectResponse,
  MomentDirectiveRequest,
  MomentDirectiveResponse,
} from '@42ch/nexus-contracts';
import {
  expectedPlatformPackage,
  fenceCompatibilityPair,
  loadNativeBinding,
  loadNodePath,
  readBundledCompatibility,
  readCompatibilityManifest,
  readPackageManifest,
  type NativeCoreBinding,
} from './loader.js';
import { parseJsonBuffer } from './json.js';
export { isNativeCoreErrorCode, parseNativeCoreError } from './errors.js';
import {
  CORE_CHANGES_REQUEST_SHAPE,
  CORE_HOST_QUERY_SHAPE,
  NATIVE_OPEN_OPTIONS_SHAPE,
  PROVIDER_CALL_SHAPE,
  WORLD_KB_PATCH_ENTITY_SHAPE,
  assertSafeInteger,
  encodeWireBuffer,
  stringifyWire,
} from './validate.js';

function unpackTsfnJson(...args: unknown[]): string {
  const payload = args.length > 1 ? args[1] : args[0];
  if (typeof payload !== 'string') {
    throw new Error('provider callback payload must be a JSON string');
  }
  return payload;
}

export interface ProviderCallbacks {
  call(request: ProviderCall): Promise<ProviderReply>;
  next(operationId: string, maxEvents: number, maxBytes: number): Promise<ProviderEventBatch>;
}

export type PrincipalHandle = string & { readonly __brand: unique symbol };

export interface NativeCore {
  activePrincipal(): Promise<PrincipalHandle>;
  worldKbGraph(
    principal: PrincipalHandle,
    worldId: string,
    includeSuggested: boolean,
  ): Promise<WorldKbGraphResponse>;
  patchWorldKbEntity(
    principal: PrincipalHandle,
    worldId: string,
    request: WorldKbPatchEntityRequest,
  ): Promise<WorldKbPatchEntityResponse>;
  worldKbCandidates(
    principal: PrincipalHandle,
    worldId: string,
    limit?: number,
    cursor?: string,
  ): Promise<WorldKbCandidatesResponse>;
  hostQuery(request: CoreHostQuery): Promise<CoreHostQueryResponse>;
  changes(principal: PrincipalHandle, request: CoreChangesRequest): Promise<CoreChangesResponse>;
  providerCall(request: ProviderCall): Promise<ProviderReply>;
  nextProviderEvents(
    operationId: string,
    maxEvents: number,
    maxBytes: number,
  ): Promise<ProviderEventBatch>;
  close(): Promise<CoreCloseReport>;
  // ── P5-T1 World / Work / content / knowledge family surface ──────────────
  // Owned payloads cross as JSON; the stored Principal is minted natively and
  // the handle only proves it. Every payload schema is the wire SSOT.
  narrativeListWorlds(principal: PrincipalHandle): Promise<NarrativeWorldsListResponse>;
  narrativeGetWorld(principal: PrincipalHandle, worldId: string): Promise<NarrativeWorldResponse>;
  createWorld(principal: PrincipalHandle, request: CreateWorldRequest): Promise<CreateWorldResponse>;
  deleteWorld(principal: PrincipalHandle, worldId: string): Promise<void>;
  promoteWorldKbCandidate(
    principal: PrincipalHandle,
    worldId: string,
    request: WorldKbPromoteCandidateRequest,
  ): Promise<WorldKbPromoteCandidateResponse>;
  patchWorldKbRelationship(
    principal: PrincipalHandle,
    worldId: string,
    request: WorldKbPatchRelationshipRequest,
  ): Promise<WorldKbPatchRelationshipResponse>;
  worldKbKeyBlockState(
    principal: PrincipalHandle,
    worldId: string,
    keyBlockId: string,
  ): Promise<WorldKbKeyBlockStateResponse>;
  createWorldFork(
    principal: PrincipalHandle,
    worldId: string,
    request: CreateForkRequest,
  ): Promise<CreateForkResponse>;
  exportWorldPack(
    principal: PrincipalHandle,
    worldId: string,
    request: PackExportRequest,
  ): Promise<PackExportResponse>;
  importWorldPack(
    principal: PrincipalHandle,
    worldId: string,
    request: PackImportRequest,
  ): Promise<PackImportResponse>;
  listWorldRules(principal: PrincipalHandle, worldId: string): Promise<WorldRulesListResponse>;
  createWorldRule(
    principal: PrincipalHandle,
    worldId: string,
    request: WorldRuleCreateRequest,
  ): Promise<WorldRuleResponse>;
  updateWorldRule(
    principal: PrincipalHandle,
    worldId: string,
    ruleId: string,
    request: WorldRuleUpdateRequest,
  ): Promise<WorldRuleResponse>;
  listWorldFindings(principal: PrincipalHandle, worldId: string): Promise<WorldFindingsListResponse>;
  timelineOverview(principal: PrincipalHandle, query: CoreTimelineOverviewQuery): Promise<TimelineOverviewResponse>;
  listTimelineEvents(
    principal: PrincipalHandle,
    worldId: string,
    query: CoreTimelineEventsQuery,
  ): Promise<ListTimelineEventsResponse>;
  listWorks(principal: PrincipalHandle, query: ListWorksQuery): Promise<ListWorksResponse>;
  getWork(principal: PrincipalHandle, workId: string): Promise<WorkDetailResponse>;
  /** `[created, response]` — the adapter maps the flag to 201/200. */
  createWork(
    principal: PrincipalHandle,
    request: CreateWorkRequest,
  ): Promise<[boolean, CreateWorkResponse]>;
  patchWork(
    principal: PrincipalHandle,
    workId: string,
    request: PatchWorkRequest,
  ): Promise<WorkDetailResponse>;
  deleteWork(principal: PrincipalHandle, workId: string): Promise<void>;
  appendWorkInspiration(
    principal: PrincipalHandle,
    workId: string,
    request: AppendInspirationRequest,
  ): Promise<AppendInspirationResponse>;
  setWorkPoolActive(
    principal: PrincipalHandle,
    request: WorkPoolSetActiveRequest,
  ): Promise<WorkPoolEntry>;
  releaseWorkCompletionLock(
    principal: PrincipalHandle,
    workId: string,
    request: ReleaseCompletionLockRequest,
  ): Promise<WorkDetailResponse>;
  reconcileWorkChapters(
    principal: PrincipalHandle,
    workId: string,
    query: { dry_run?: boolean },
  ): Promise<WorkReconcileReport>;
  selectWork(principal: PrincipalHandle, workId: string): Promise<CoreWorkSelection>;
  listWorkPool(principal: PrincipalHandle, query: WorkPoolListQuery): Promise<WorkPoolListResponse>;
  promoteWorkPoolEntry(
    principal: PrincipalHandle,
    request: WorkPoolPromoteRequest,
  ): Promise<WorkPoolEntry>;
  archiveWorkPoolEntry(
    principal: PrincipalHandle,
    request: WorkPoolArchiveRequest,
  ): Promise<WorkPoolEntry>;
  addWorkInspiration(
    principal: PrincipalHandle,
    request: WorkInspirationAddRequest,
  ): Promise<WorkInspirationAddResponse>;
  listWorkInspiration(
    principal: PrincipalHandle,
    query: WorkInspirationListQuery,
  ): Promise<WorkInspirationListResponse>;
  promoteWorkInspiration(
    principal: PrincipalHandle,
    request: WorkInspirationPromoteRequest,
  ): Promise<WorkInspirationPromoteResponse>;
  archiveWorkInspiration(
    principal: PrincipalHandle,
    request: WorkInspirationArchiveRequest,
  ): Promise<WorkInspirationItem>;
  listChapters(
    principal: PrincipalHandle,
    workId: string,
    query: ListChaptersQuery,
  ): Promise<ListChaptersResponse>;
  chapterDetail(
    principal: PrincipalHandle,
    workId: string,
    chapterId: string,
    query: CoreChapterContentQuery,
  ): Promise<ChapterDetail>;
  chapterOutline(
    principal: PrincipalHandle,
    workId: string,
    chapterId: string,
    query: CoreChapterContentQuery,
  ): Promise<ChapterOutline>;
  chapterBody(
    principal: PrincipalHandle,
    workId: string,
    chapterId: string,
    query: CoreChapterContentQuery,
  ): Promise<ChapterBody>;
  patchChapter(
    principal: PrincipalHandle,
    workId: string,
    chapterId: string,
    query: CoreChapterContentQuery,
    request: PatchChapterRequest,
  ): Promise<ChapterDetail>;
  getWorkOutline(principal: PrincipalHandle, workId: string): Promise<WorkOutline>;
  patchOutlineStructure(
    principal: PrincipalHandle,
    workId: string,
    request: OutlinePatchStructureRequest,
  ): Promise<OutlinePatchResponse>;
  patchOutlineChapter(
    principal: PrincipalHandle,
    workId: string,
    chapterId: string,
    request: OutlinePatchChapterRequest,
  ): Promise<OutlinePatchResponse>;
  patchTimelineEvent(
    principal: PrincipalHandle,
    workId: string,
    request: TimelinePatchEventRequest,
  ): Promise<OutlinePatchResponse>;
  listKbEntries(principal: PrincipalHandle, query: ListKbEntriesQuery): Promise<ListKbEntriesResponse>;
  addKbEntry(principal: PrincipalHandle, request: AddKbEntryRequest): Promise<AddKbEntryResponse>;
  getKbEntry(principal: PrincipalHandle, entryId: string): Promise<GetKbEntryResponse>;
  deleteKbEntry(principal: PrincipalHandle, entryId: string): Promise<DeleteKbEntryResponse>;
  createFinding(
    principal: PrincipalHandle,
    workId: string,
    request: CreateFindingRequest,
  ): Promise<FindingDetailResponse>;
  createFindingFromReview(
    principal: PrincipalHandle,
    workId: string,
    request: CreateFindingRequest,
  ): Promise<FindingDetailResponse>;
  listFindings(
    principal: PrincipalHandle,
    workId: string,
    query: ListFindingsQuery,
  ): Promise<ListFindingsResponse>;
  getWorkFinding(
    principal: PrincipalHandle,
    workId: string,
    findingId: string,
  ): Promise<FindingDetailResponse>;
  getFinding(principal: PrincipalHandle, findingId: string): Promise<FindingDetailResponse>;
  updateFinding(
    principal: PrincipalHandle,
    findingId: string,
    request: UpdateFindingRequest,
  ): Promise<FindingDetailResponse>;
  deleteFinding(principal: PrincipalHandle, findingId: string): Promise<void>;
  batchUpdateFindings(
    principal: PrincipalHandle,
    request: BatchUpdateFindingsRequest,
  ): Promise<BatchUpdateFindingsResponse>;
  listStaleFindings(
    principal: PrincipalHandle,
    thresholdSeconds: number,
  ): Promise<StaleFindingsResponse>;
  pruneFindings(
    principal: PrincipalHandle,
    olderThanDays: number | null,
    dryRun: boolean,
  ): Promise<FindingsPruneResponse>;
  getReadingProgress(
    principal: PrincipalHandle,
    query: ReadingProgressQuery,
  ): Promise<ReadingProgressResponse>;
  putReadingProgress(
    principal: PrincipalHandle,
    workId: string,
    request: ReadingProgressRequest,
  ): Promise<ReadingProgressResponse>;
  deleteReadingProgress(principal: PrincipalHandle, query: ReadingProgressQuery): Promise<void>;
  listAnnotations(
    principal: PrincipalHandle,
    query: ReadingAnnotationListQuery,
  ): Promise<ReadingAnnotationListResponse>;
  createAnnotation(
    principal: PrincipalHandle,
    request: ReadingAnnotationCreateRequest,
  ): Promise<ReadingAnnotation>;
  patchAnnotation(
    principal: PrincipalHandle,
    annotationId: string,
    request: ReadingAnnotationPatchRequest,
  ): Promise<ReadingAnnotation>;
  deleteAnnotation(principal: PrincipalHandle, annotationId: string): Promise<void>;
  listReferences(principal: PrincipalHandle): Promise<ReferenceListResponse>;
  getReference(principal: PrincipalHandle, referenceId: string): Promise<ReferenceGetResponse>;
  // ── P5-T2 Actor / memory / context family surface ─────────────────────────
  // Same owned-JSON-payload policy as the P5-T1 block above: the stored
  // Principal is minted natively, the handle only proves it, and every
  // payload schema is the wire SSOT.
  listCharacters(principal: PrincipalHandle, query: ListCharactersQuery): Promise<ListCharactersResponse>;
  createCharacter(
    principal: PrincipalHandle,
    request: CreateCharacterRequest,
  ): Promise<CreateCharacterResponse>;
  getCharacter(principal: PrincipalHandle, characterId: string): Promise<CharacterDetail>;
  patchCharacter(
    principal: PrincipalHandle,
    characterId: string,
    request: UpdateCharacterRequest,
  ): Promise<CharacterDetail>;
  archiveCharacter(
    principal: PrincipalHandle,
    characterId: string,
    request: CharacterLifecycleRequest,
  ): Promise<CharacterDetail>;
  restoreCharacter(
    principal: PrincipalHandle,
    characterId: string,
    request: CharacterLifecycleRequest,
  ): Promise<CharacterDetail>;
  addCharacterBinding(
    principal: PrincipalHandle,
    characterId: string,
    request: AddCharacterBindingRequest,
  ): Promise<AddCharacterBindingResponse>;
  listCharacterBindings(
    principal: PrincipalHandle,
    characterId: string,
    query: ListCharacterBindingsQuery,
  ): Promise<ListCharacterBindingsResponse>;
  getCharacterBinding(
    principal: PrincipalHandle,
    characterId: string,
    bindingId: string,
  ): Promise<CharacterBindingDetail>;
  patchCharacterBinding(
    principal: PrincipalHandle,
    characterId: string,
    bindingId: string,
    request: UpdateCharacterBindingRequest,
  ): Promise<CharacterBindingDetail>;
  removeCharacterBinding(principal: PrincipalHandle, characterId: string, bindingId: string): Promise<void>;
  actorKnowledgeView(principal: PrincipalHandle, request: ViewRequest): Promise<ViewResponse>;
  addActorKnowledgeEntry(
    principal: PrincipalHandle,
    request: AddKnowledgeEntryRequest,
  ): Promise<AddKnowledgeEntryResponse>;
  listCharacterKnowledge(
    principal: PrincipalHandle,
    characterId: string,
    query: ListCharacterKnowledgeQuery,
  ): Promise<ListCharacterKnowledgeResponse>;
  getKnowledgeEntry(
    principal: PrincipalHandle,
    characterId: string,
    entryId: string,
  ): Promise<KnowledgeEntryDetail>;
  patchKnowledgeEntry(
    principal: PrincipalHandle,
    characterId: string,
    entryId: string,
    request: UpdateKnowledgeEntryRequest,
  ): Promise<KnowledgeEntryDetail>;
  deleteKnowledgeEntry(
    principal: PrincipalHandle,
    characterId: string,
    entryId: string,
    expectedRevision: number,
  ): Promise<void>;
  listCreators(query: ListCreatorsQuery): Promise<ListCreatorsResponse>;
  createCreator(displayName: string): Promise<CreatorDetail>;
  getCreator(creatorId: string): Promise<CreatorDetail>;
  patchCreator(creatorId: string, displayName?: string): Promise<CreatorDetail>;
  setActiveCreator(request: SetActiveCreatorRequest): Promise<SetActiveCreatorResponse>;
  getActiveCreator(): Promise<ActiveCreatorResponse>;
  logoutCreator(creatorId: string): Promise<LogoutResponse>;
  captureCharacterPendingReview(
    principal: PrincipalHandle,
    characterId: string,
    request: CaptureCharacterPendingReviewRequest,
  ): Promise<CaptureCharacterPendingReviewResponse>;
  listCharacterPendingReviews(
    principal: PrincipalHandle,
    characterId: string,
    query: ListCharacterPendingReviewsQuery,
  ): Promise<ListCharacterPendingReviewsResponse>;
  countCharacterPendingReviews(
    principal: PrincipalHandle,
    characterId: string,
    query: CountCharacterPendingReviewsQuery,
  ): Promise<CountCharacterPendingReviewsResponse>;
  deleteCharacterPendingReview(
    principal: PrincipalHandle,
    characterId: string,
    pendingId: string,
  ): Promise<DeleteCharacterPendingReviewResponse>;
  reviewCharacterMemory(
    principal: PrincipalHandle,
    characterId: string,
    request: ReviewCharacterMemoryRequest,
  ): Promise<ReviewCharacterMemoryResponse>;
  listCharacterMemoryFragments(
    principal: PrincipalHandle,
    characterId: string,
    query: ListCharacterMemoryFragmentsQuery,
  ): Promise<ListCharacterMemoryFragmentsResponse>;
  promoteCharacterFragment(
    principal: PrincipalHandle,
    characterId: string,
    fragmentId: string,
    request: PromoteCharacterFragmentRequest,
  ): Promise<PromoteCharacterFragmentResponse>;
  reflectCharacterSoul(
    principal: PrincipalHandle,
    characterId: string,
    request: CharacterSoulNarrativeRequest,
  ): Promise<CharacterSoulNarrativeResponse>;
  recordCharacterTom(
    principal: PrincipalHandle,
    characterId: string,
    request: RecordCharacterTomRequest,
  ): Promise<RecordCharacterTomResponse>;
  listCharacterTom(
    principal: PrincipalHandle,
    characterId: string,
    query: ListCharacterTomQuery,
  ): Promise<ListCharacterTomResponse>;
  listPendingReviews(principal: PrincipalHandle, query: ListPendingReviewsQuery): Promise<ListPendingReviewsResponse>;
  countPendingReviews(principal: PrincipalHandle): Promise<CountPendingReviewsResponse>;
  deletePendingReview(principal: PrincipalHandle, pendingId: string): Promise<DeletePendingReviewResponse>;
  reviewMemory(principal: PrincipalHandle, request: ReviewRequest): Promise<ReviewResponse>;
  listMemoryFragments(
    principal: PrincipalHandle,
    query: ListMemoryFragmentsQuery,
  ): Promise<ListMemoryFragmentsResponse>;
  reflectCreatorSoul(
    principal: PrincipalHandle,
    request: SoulNarrativeRequest,
  ): Promise<SoulNarrativeResponse>;
  inspectMoment(principal: PrincipalHandle, request: MomentInspectRequest): Promise<MomentInspectResponse>;
  momentDirective(
    principal: PrincipalHandle,
    request: MomentDirectiveRequest,
  ): Promise<MomentDirectiveResponse>;
}

function wrapCore(inner: NativeCoreBinding): NativeCore {
  return {
    async activePrincipal() {
      return (await inner.activePrincipal()) as PrincipalHandle;
    },
    async worldKbGraph(principal, worldId, includeSuggested) {
      return parseJsonBuffer(await inner.worldKbGraph(principal, worldId, includeSuggested));
    },
    async patchWorldKbEntity(principal, worldId, request) {
      return parseJsonBuffer(
        await inner.patchWorldKbEntity(
          principal,
          worldId,
          encodeWireBuffer(request, WORLD_KB_PATCH_ENTITY_SHAPE, 'request'),
        ),
      );
    },
    async worldKbCandidates(principal, worldId, limit, cursor) {
      return parseJsonBuffer(
        await inner.worldKbCandidates(
          principal,
          worldId,
          assertSafeInteger(limit ?? null, 'limit'),
          cursor ?? null,
        ),
      );
    },
    async hostQuery(request) {
      return parseJsonBuffer(
        await inner.hostQuery(encodeWireBuffer(request, CORE_HOST_QUERY_SHAPE, 'request')),
      );
    },
    async changes(principal, request) {
      return parseJsonBuffer(
        await inner.changes(
          principal,
          encodeWireBuffer(request, CORE_CHANGES_REQUEST_SHAPE, 'request'),
        ),
      );
    },
    async providerCall(request) {
      return parseJsonBuffer(
        await inner.providerCall(encodeWireBuffer(request, PROVIDER_CALL_SHAPE, 'request')),
      );
    },
    async nextProviderEvents(operationId, maxEvents, maxBytes) {
      return parseJsonBuffer(await inner.nextProviderEvents(operationId, maxEvents, maxBytes));
    },
    async close() {
      return parseJsonBuffer(await inner.close());
    },
    ...wrapDomainSurface(inner),
  };
}

/**
 * World / Work / content / knowledge family surface (P5-T1).
 *
 * Owned payloads cross as JSON buffers. There is deliberately no handwritten
 * TS shape table for these payloads: the generated contract types bind every
 * call site (the interface above), `assertSafeNumbers` rejects integers the
 * exactly-representable wire policy excludes, and the native side re-parses
 * every buffer into the generated DTO (`deny_unknown_fields`), so the schema
 * stays the single shape authority. Duplicating ~60 shapes here would be the
 * second handwritten shape set the plan forbids.
 */
/**
 * The domain-method subset of `NativeCore`, spread into the facade object
 * that carries the pre-existing methods. The `Pick` gives every wrapper
 * method its parameter types from the one interface declaration — no second
 * signature table.
 */
type DomainSurface = Pick<
  NativeCore,
  | 'narrativeListWorlds'
  | 'narrativeGetWorld'
  | 'createWorld'
  | 'deleteWorld'
  | 'promoteWorldKbCandidate'
  | 'patchWorldKbRelationship'
  | 'worldKbKeyBlockState'
  | 'createWorldFork'
  | 'exportWorldPack'
  | 'importWorldPack'
  | 'listWorldRules'
  | 'createWorldRule'
  | 'updateWorldRule'
  | 'listWorldFindings'
  | 'timelineOverview'
  | 'listTimelineEvents'
  | 'listWorks'
  | 'getWork'
  | 'createWork'
  | 'patchWork'
  | 'deleteWork'
  | 'appendWorkInspiration'
  | 'setWorkPoolActive'
  | 'releaseWorkCompletionLock'
  | 'reconcileWorkChapters'
  | 'selectWork'
  | 'listWorkPool'
  | 'promoteWorkPoolEntry'
  | 'archiveWorkPoolEntry'
  | 'addWorkInspiration'
  | 'listWorkInspiration'
  | 'promoteWorkInspiration'
  | 'archiveWorkInspiration'
  | 'listChapters'
  | 'chapterDetail'
  | 'chapterOutline'
  | 'chapterBody'
  | 'patchChapter'
  | 'getWorkOutline'
  | 'patchOutlineStructure'
  | 'patchOutlineChapter'
  | 'patchTimelineEvent'
  | 'listKbEntries'
  | 'addKbEntry'
  | 'getKbEntry'
  | 'deleteKbEntry'
  | 'createFinding'
  | 'createFindingFromReview'
  | 'listFindings'
  | 'getWorkFinding'
  | 'getFinding'
  | 'updateFinding'
  | 'deleteFinding'
  | 'batchUpdateFindings'
  | 'listStaleFindings'
  | 'pruneFindings'
  | 'getReadingProgress'
  | 'putReadingProgress'
  | 'deleteReadingProgress'
  | 'listAnnotations'
  | 'createAnnotation'
  | 'patchAnnotation'
  | 'deleteAnnotation'
  | 'listReferences'
  | 'getReference'
  | 'listCharacters'
  | 'createCharacter'
  | 'getCharacter'
  | 'patchCharacter'
  | 'archiveCharacter'
  | 'restoreCharacter'
  | 'addCharacterBinding'
  | 'listCharacterBindings'
  | 'getCharacterBinding'
  | 'patchCharacterBinding'
  | 'removeCharacterBinding'
  | 'actorKnowledgeView'
  | 'addActorKnowledgeEntry'
  | 'listCharacterKnowledge'
  | 'getKnowledgeEntry'
  | 'patchKnowledgeEntry'
  | 'deleteKnowledgeEntry'
  | 'listCreators'
  | 'createCreator'
  | 'getCreator'
  | 'patchCreator'
  | 'setActiveCreator'
  | 'getActiveCreator'
  | 'logoutCreator'
  | 'captureCharacterPendingReview'
  | 'listCharacterPendingReviews'
  | 'countCharacterPendingReviews'
  | 'deleteCharacterPendingReview'
  | 'reviewCharacterMemory'
  | 'listCharacterMemoryFragments'
  | 'promoteCharacterFragment'
  | 'reflectCharacterSoul'
  | 'recordCharacterTom'
  | 'listCharacterTom'
  | 'listPendingReviews'
  | 'countPendingReviews'
  | 'deletePendingReview'
  | 'reviewMemory'
  | 'listMemoryFragments'
  | 'reflectCreatorSoul'
  | 'inspectMoment'
  | 'momentDirective'
>;

function wrapDomainSurface(inner: NativeCoreBinding): DomainSurface {
  const wire = (value: unknown, label: string): Uint8Array =>
    encodeWireBuffer(value, undefined, label);
  const json = async <T>(payload: Uint8Array | Promise<Uint8Array>): Promise<T> =>
    parseJsonBuffer<T>(await payload);
  return {
    async narrativeListWorlds(principal) {
      return json(await inner.narrativeListWorlds(principal));
    },
    async narrativeGetWorld(principal, worldId) {
      return json(await inner.narrativeGetWorld(principal, worldId));
    },
    async createWorld(principal, request) {
      return json(await inner.createWorld(principal, wire(request, 'request')));
    },
    async deleteWorld(principal, worldId) {
      await inner.deleteWorld(principal, worldId);
    },
    async promoteWorldKbCandidate(principal, worldId, request) {
      return json(await inner.promoteWorldKbCandidate(principal, worldId, wire(request, 'request')));
    },
    async patchWorldKbRelationship(principal, worldId, request) {
      return json(
        await inner.patchWorldKbRelationship(principal, worldId, wire(request, 'request')),
      );
    },
    async worldKbKeyBlockState(principal, worldId, keyBlockId) {
      return json(await inner.worldKbKeyBlockState(principal, worldId, keyBlockId));
    },
    async createWorldFork(principal, worldId, request) {
      return json(await inner.createWorldFork(principal, worldId, wire(request, 'request')));
    },
    async exportWorldPack(principal, worldId, request) {
      return json(await inner.exportWorldPack(principal, worldId, wire(request, 'request')));
    },
    async importWorldPack(principal, worldId, request) {
      return json(await inner.importWorldPack(principal, worldId, wire(request, 'request')));
    },
    async listWorldRules(principal, worldId) {
      return json(await inner.listWorldRules(principal, worldId));
    },
    async createWorldRule(principal, worldId, request) {
      return json(await inner.createWorldRule(principal, worldId, wire(request, 'request')));
    },
    async updateWorldRule(principal, worldId, ruleId, request) {
      return json(
        await inner.updateWorldRule(principal, worldId, ruleId, wire(request, 'request')),
      );
    },
    async listWorldFindings(principal, worldId) {
      return json(await inner.listWorldFindings(principal, worldId));
    },
    async timelineOverview(principal, query) {
      return json(await inner.timelineOverview(principal, wire(query, 'query')));
    },
    async listTimelineEvents(principal, worldId, query) {
      return json(await inner.listTimelineEvents(principal, worldId, wire(query, 'query')));
    },
    async listWorks(principal, query) {
      return json(await inner.listWorks(principal, wire(query, 'query')));
    },
    async getWork(principal, workId) {
      return json(await inner.getWork(principal, workId));
    },
    async createWork(principal, request) {
      return json(await inner.createWork(principal, wire(request, 'request')));
    },
    async patchWork(principal, workId, request) {
      return json(await inner.patchWork(principal, workId, wire(request, 'request')));
    },
    async deleteWork(principal, workId) {
      await inner.deleteWork(principal, workId);
    },
    async appendWorkInspiration(principal, workId, request) {
      return json(
        await inner.appendWorkInspiration(principal, workId, wire(request, 'request')),
      );
    },
    async setWorkPoolActive(principal, request) {
      return json(await inner.setWorkPoolActive(principal, wire(request, 'request')));
    },
    async releaseWorkCompletionLock(principal, workId, request) {
      return json(
        await inner.releaseWorkCompletionLock(principal, workId, wire(request, 'request')),
      );
    },
    async reconcileWorkChapters(principal, workId, query) {
      return json(await inner.reconcileWorkChapters(principal, workId, wire(query, 'query')));
    },
    async selectWork(principal, workId) {
      return json(await inner.selectWork(principal, workId));
    },
    async listWorkPool(principal, query) {
      return json(await inner.listWorkPool(principal, wire(query, 'query')));
    },
    async promoteWorkPoolEntry(principal, request) {
      return json(await inner.promoteWorkPoolEntry(principal, wire(request, 'request')));
    },
    async archiveWorkPoolEntry(principal, request) {
      return json(await inner.archiveWorkPoolEntry(principal, wire(request, 'request')));
    },
    async addWorkInspiration(principal, request) {
      return json(await inner.addWorkInspiration(principal, wire(request, 'request')));
    },
    async listWorkInspiration(principal, query) {
      return json(await inner.listWorkInspiration(principal, wire(query, 'query')));
    },
    async promoteWorkInspiration(principal, request) {
      return json(await inner.promoteWorkInspiration(principal, wire(request, 'request')));
    },
    async archiveWorkInspiration(principal, request) {
      return json(await inner.archiveWorkInspiration(principal, wire(request, 'request')));
    },
    async listChapters(principal, workId, query) {
      return json(await inner.listChapters(principal, workId, wire(query, 'query')));
    },
    async chapterDetail(principal, workId, chapterId, query) {
      return json(await inner.chapterDetail(principal, workId, chapterId, wire(query, 'query')));
    },
    async chapterOutline(principal, workId, chapterId, query) {
      return json(await inner.chapterOutline(principal, workId, chapterId, wire(query, 'query')));
    },
    async chapterBody(principal, workId, chapterId, query) {
      return json(await inner.chapterBody(principal, workId, chapterId, wire(query, 'query')));
    },
    async patchChapter(principal, workId, chapterId, query, request) {
      return json(
        await inner.patchChapter(
          principal,
          workId,
          chapterId,
          wire(query, 'query'),
          wire(request, 'request'),
        ),
      );
    },
    async getWorkOutline(principal, workId) {
      return json(await inner.getWorkOutline(principal, workId));
    },
    async patchOutlineStructure(principal, workId, request) {
      return json(
        await inner.patchOutlineStructure(principal, workId, wire(request, 'request')),
      );
    },
    async patchOutlineChapter(principal, workId, chapterId, request) {
      return json(
        await inner.patchOutlineChapter(principal, workId, chapterId, wire(request, 'request')),
      );
    },
    async patchTimelineEvent(principal, workId, request) {
      return json(await inner.patchTimelineEvent(principal, workId, wire(request, 'request')));
    },
    async listKbEntries(principal, query) {
      return json(await inner.listKbEntries(principal, wire(query, 'query')));
    },
    async addKbEntry(principal, request) {
      return json(await inner.addKbEntry(principal, wire(request, 'request')));
    },
    async getKbEntry(principal, entryId) {
      return json(await inner.getKbEntry(principal, entryId));
    },
    async deleteKbEntry(principal, entryId) {
      return json(await inner.deleteKbEntry(principal, entryId));
    },
    async createFinding(principal, workId, request) {
      return json(await inner.createFinding(principal, workId, wire(request, 'request')));
    },
    async createFindingFromReview(principal, workId, request) {
      return json(await inner.createFindingFromReview(principal, workId, wire(request, 'request')));
    },
    async listFindings(principal, workId, query) {
      return json(await inner.listFindings(principal, workId, wire(query, 'query')));
    },
    async getWorkFinding(principal, workId, findingId) {
      return json(await inner.getWorkFinding(principal, workId, findingId));
    },
    async getFinding(principal, findingId) {
      return json(await inner.getFinding(principal, findingId));
    },
    async updateFinding(principal, findingId, request) {
      return json(await inner.updateFinding(principal, findingId, wire(request, 'request')));
    },
    async deleteFinding(principal, findingId) {
      await inner.deleteFinding(principal, findingId);
    },
    async batchUpdateFindings(principal, request) {
      return json(await inner.batchUpdateFindings(principal, wire(request, 'request')));
    },
    async listStaleFindings(principal, thresholdSeconds) {
      return json(await inner.listStaleFindings(principal, thresholdSeconds));
    },
    async pruneFindings(principal, olderThanDays, dryRun) {
      return json(await inner.pruneFindings(principal, olderThanDays, dryRun));
    },
    async getReadingProgress(principal, query) {
      return json(await inner.getReadingProgress(principal, wire(query, 'query')));
    },
    async putReadingProgress(principal, workId, request) {
      return json(await inner.putReadingProgress(principal, workId, wire(request, 'request')));
    },
    async deleteReadingProgress(principal, query) {
      await inner.deleteReadingProgress(principal, wire(query, 'query'));
    },
    async listAnnotations(principal, query) {
      return json(await inner.listAnnotations(principal, wire(query, 'query')));
    },
    async createAnnotation(principal, request) {
      return json(await inner.createAnnotation(principal, wire(request, 'request')));
    },
    async patchAnnotation(principal, annotationId, request) {
      return json(await inner.patchAnnotation(principal, annotationId, wire(request, 'request')));
    },
    async deleteAnnotation(principal, annotationId) {
      await inner.deleteAnnotation(principal, annotationId);
    },
    async listReferences(principal) {
      return json(await inner.listReferences(principal));
    },
    async getReference(principal, referenceId) {
      return json(await inner.getReference(principal, referenceId));
    },
    // ── P5-T2 Actor / memory / context family surface ────────────────────────
    async listCharacters(principal, query) {
      return json(await inner.listCharacters(principal, wire(query, 'query')));
    },
    async createCharacter(principal, request) {
      return json(await inner.createCharacter(principal, wire(request, 'request')));
    },
    async getCharacter(principal, characterId) {
      return json(await inner.getCharacter(principal, characterId));
    },
    async patchCharacter(principal, characterId, request) {
      return json(await inner.patchCharacter(principal, characterId, wire(request, 'request')));
    },
    async archiveCharacter(principal, characterId, request) {
      return json(await inner.archiveCharacter(principal, characterId, wire(request, 'request')));
    },
    async restoreCharacter(principal, characterId, request) {
      return json(await inner.restoreCharacter(principal, characterId, wire(request, 'request')));
    },
    async addCharacterBinding(principal, characterId, request) {
      return json(
        await inner.addCharacterBinding(principal, characterId, wire(request, 'request')),
      );
    },
    async listCharacterBindings(principal, characterId, query) {
      return json(
        await inner.listCharacterBindings(principal, characterId, wire(query, 'query')),
      );
    },
    async getCharacterBinding(principal, characterId, bindingId) {
      return json(await inner.getCharacterBinding(principal, characterId, bindingId));
    },
    async patchCharacterBinding(principal, characterId, bindingId, request) {
      return json(
        await inner.patchCharacterBinding(
          principal,
          characterId,
          bindingId,
          wire(request, 'request'),
        ),
      );
    },
    async removeCharacterBinding(principal, characterId, bindingId) {
      await inner.removeCharacterBinding(principal, characterId, bindingId);
    },
    async actorKnowledgeView(principal, request) {
      return json(await inner.actorKnowledgeView(principal, wire(request, 'request')));
    },
    async addActorKnowledgeEntry(principal, request) {
      return json(await inner.addActorKnowledgeEntry(principal, wire(request, 'request')));
    },
    async listCharacterKnowledge(principal, characterId, query) {
      return json(
        await inner.listCharacterKnowledge(principal, characterId, wire(query, 'query')),
      );
    },
    async getKnowledgeEntry(principal, characterId, entryId) {
      return json(await inner.getKnowledgeEntry(principal, characterId, entryId));
    },
    async patchKnowledgeEntry(principal, characterId, entryId, request) {
      return json(
        await inner.patchKnowledgeEntry(
          principal,
          characterId,
          entryId,
          wire(request, 'request'),
        ),
      );
    },
    async deleteKnowledgeEntry(principal, characterId, entryId, expectedRevision) {
      await inner.deleteKnowledgeEntry(
        principal,
        characterId,
        entryId,
        expectedRevision,
      );
    },
    async listCreators(query) {
      return json(await inner.listCreators(wire(query, 'query')));
    },
    async createCreator(displayName) {
      return json(await inner.createCreator(displayName));
    },
    async getCreator(creatorId) {
      return json(await inner.getCreator(creatorId));
    },
    async patchCreator(creatorId, displayName) {
      return json(await inner.patchCreator(creatorId, displayName ?? null));
    },
    async setActiveCreator(request) {
      return json(await inner.setActiveCreator(wire(request, 'request')));
    },
    async getActiveCreator() {
      return json(await inner.getActiveCreator());
    },
    async logoutCreator(creatorId) {
      return json(await inner.logoutCreator(creatorId));
    },
    async captureCharacterPendingReview(principal, characterId, request) {
      return json(
        await inner.captureCharacterPendingReview(
          principal,
          characterId,
          wire(request, 'request'),
        ),
      );
    },
    async listCharacterPendingReviews(principal, characterId, query) {
      return json(
        await inner.listCharacterPendingReviews(
          principal,
          characterId,
          wire(query, 'query'),
        ),
      );
    },
    async countCharacterPendingReviews(principal, characterId, query) {
      return json(
        await inner.countCharacterPendingReviews(
          principal,
          characterId,
          wire(query, 'query'),
        ),
      );
    },
    async deleteCharacterPendingReview(principal, characterId, pendingId) {
      return json(await inner.deleteCharacterPendingReview(principal, characterId, pendingId));
    },
    async reviewCharacterMemory(principal, characterId, request) {
      return json(
        await inner.reviewCharacterMemory(principal, characterId, wire(request, 'request')),
      );
    },
    async listCharacterMemoryFragments(principal, characterId, query) {
      return json(
        await inner.listCharacterMemoryFragments(
          principal,
          characterId,
          wire(query, 'query'),
        ),
      );
    },
    async promoteCharacterFragment(principal, characterId, fragmentId, request) {
      return json(
        await inner.promoteCharacterFragment(
          principal,
          characterId,
          fragmentId,
          wire(request, 'request'),
        ),
      );
    },
    async reflectCharacterSoul(principal, characterId, request) {
      return json(
        await inner.reflectCharacterSoul(principal, characterId, wire(request, 'request')),
      );
    },
    async recordCharacterTom(principal, characterId, request) {
      return json(await inner.recordCharacterTom(principal, characterId, wire(request, 'request')));
    },
    async listCharacterTom(principal, characterId, query) {
      return json(await inner.listCharacterTom(principal, characterId, wire(query, 'query')));
    },
    async listPendingReviews(principal, query) {
      return json(await inner.listPendingReviews(principal, wire(query, 'query')));
    },
    async countPendingReviews(principal) {
      return json(await inner.countPendingReviews(principal));
    },
    async deletePendingReview(principal, pendingId) {
      return json(await inner.deletePendingReview(principal, pendingId));
    },
    async reviewMemory(principal, request) {
      return json(await inner.reviewMemory(principal, wire(request, 'request')));
    },
    async listMemoryFragments(principal, query) {
      return json(await inner.listMemoryFragments(principal, wire(query, 'query')));
    },
    async reflectCreatorSoul(principal, request) {
      return json(await inner.reflectCreatorSoul(principal, wire(request, 'request')));
    },
    async inspectMoment(principal, request) {
      return json(await inner.inspectMoment(principal, wire(request, 'request')));
    },
    async momentDirective(principal, request) {
      return json(await inner.momentDirective(principal, wire(request, 'request')));
    },
  };
}

/** Compatibility manifest for this platform, before any DB open. */
export function nativeCompatibility(): NativeCompatibility {
  const nodePath = loadNodePath();
  const binding = loadNativeBinding();
  const manifest = readCompatibilityManifest(binding);
  const adjacent = readBundledCompatibility(nodePath);
  const target = expectedPlatformPackage();
  const pkgManifest = readPackageManifest(target.name);
  fenceCompatibilityPair(manifest, adjacent, {
    target_triple: target.targetTriple,
    package_version: pkgManifest.version,
  });
  return manifest;
}

/**
 * Open the native core. Always awaited: the frozen facade returns a Promise so
 * callers can await the open boundary.
 */
export async function openCore(
  options: NativeOpenOptions,
  providers?: ProviderCallbacks,
): Promise<NativeCore> {
  const nodePath = loadNodePath();
  const binding = loadNativeBinding();
  const manifest = readCompatibilityManifest(binding);
  const adjacent = readBundledCompatibility(nodePath);
  const target = expectedPlatformPackage();
  const pkgManifest = readPackageManifest(target.name);
  fenceCompatibilityPair(manifest, adjacent, {
    target_triple: target.targetTriple,
    package_version: pkgManifest.version,
  });

  const callbacks = providers
    ? {
        call: async (...args: unknown[]) =>
          stringifyWire(
            await providers.call(JSON.parse(unpackTsfnJson(...args))),
            undefined,
            'provider_reply',
          ),
        next: async (...args: unknown[]) => {
          const { operation_id, max_events, max_bytes } = JSON.parse(unpackTsfnJson(...args)) as {
            operation_id: string;
            max_events: number;
            max_bytes: number;
          };
          return stringifyWire(
            await providers.next(operation_id, max_events, max_bytes),
            undefined,
            'provider_events',
          );
        },
      }
    : undefined;

  const core = binding.open(
    stringifyWire(options, NATIVE_OPEN_OPTIONS_SHAPE, 'options'),
    callbacks,
  );
  return wrapCore(core);
}
