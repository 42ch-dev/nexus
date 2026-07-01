/**
 * Centralized TanStack Query keys for the Nexus Local API resources.
 *
 * Keeping keys in one place lets mutations invalidate the right query sets
 * after a write (e.g. patching a Work invalidates the work list + that work's
 * detail). Keys are hierarchical arrays so partial invalidation works:
 * `['works']` ⊃ `['works', 'list', query]` ⊃ `['works', 'detail', id]`.
 */
export const queryKeys = {
  works: {
    all: ['works'] as const,
    lists: () => [...queryKeys.works.all, 'list'] as const,
    list: (query?: object) => [...queryKeys.works.lists(), query ?? {}] as const,
    details: () => [...queryKeys.works.all, 'detail'] as const,
    detail: (workId: string) => [...queryKeys.works.details(), workId] as const,
  },
  sessions: {
    all: ['sessions'] as const,
    list: (query?: object) => [...queryKeys.sessions.all, 'list', query ?? {}] as const,
    detail: (sessionId: string) => [...queryKeys.sessions.all, 'detail', sessionId] as const,
  },
  schedules: {
    all: ['schedules'] as const,
    list: (query?: object) => [...queryKeys.schedules.all, 'list', query ?? {}] as const,
    details: () => [...queryKeys.schedules.all, 'detail'] as const,
    detail: (scheduleId: string) => [...queryKeys.schedules.details(), scheduleId] as const,
  },
  capabilities: {
    all: ['capabilities'] as const,
    list: (query?: object) => [...queryKeys.capabilities.all, 'list', query ?? {}] as const,
  },
  findings: {
    all: ['findings'] as const,
    lists: () => [...queryKeys.findings.all, 'list'] as const,
    list: (workId: string, query?: object) =>
      [...queryKeys.findings.lists(), workId, query ?? {}] as const,
  },
  presets: {
    all: ['presets'] as const,
    list: () => [...queryKeys.presets.all, 'list'] as const,
    // Detail keys staged for the V1.70 canvas Strategy surface
    // (R-V167P1-QC3-S2): getPreset/updatePreset/deletePreset operate on a
    // single preset by id. Invalidation follows the existing inline mutation
    // pattern (e.g. useReloadPreset) — `qc.invalidateQueries({ queryKey:
    // queryKeys.presets.all })` covers the list + every detail, and
    // `queryKeys.presets.detail(id)` targets one. The actual
    // `invalidateQueries` wiring lands in V1.70 when the canvas consumes these.
    details: () => [...queryKeys.presets.all, 'detail'] as const,
    detail: (presetId: string) => [...queryKeys.presets.details(), presetId] as const,
  },
  chapters: {
    all: ['chapters'] as const,
    lists: () => [...queryKeys.chapters.all, 'list'] as const,
    list: (workId: string, query?: object) =>
      [...queryKeys.chapters.lists(), workId, query ?? {}] as const,
    details: () => [...queryKeys.chapters.all, 'detail'] as const,
    detail: (workId: string, chapter: number, query?: object) =>
      [...queryKeys.chapters.details(), workId, chapter, query ?? {}] as const,
    outlines: () => [...queryKeys.chapters.all, 'outline'] as const,
    outline: (workId: string, chapter: number, query?: object) =>
      [...queryKeys.chapters.outlines(), workId, chapter, query ?? {}] as const,
    bodies: () => [...queryKeys.chapters.all, 'body'] as const,
    body: (workId: string, chapter: number, query?: object) =>
      [...queryKeys.chapters.bodies(), workId, chapter, query ?? {}] as const,
  },
  outline: {
    all: ['outline'] as const,
    detail: (workId: string) => [...queryKeys.outline.all, 'detail', workId] as const,
  },
  worldKb: {
    all: ['worldKb'] as const,
    graph: (worldId: string) => [...queryKeys.worldKb.all, 'graph', worldId] as const,
    candidates: (worldId: string, query?: object) =>
      [...queryKeys.worldKb.all, 'candidates', worldId, query ?? {}] as const,
  },
  memory: {
    all: ['memory'] as const,
    pendingLists: () => [...queryKeys.memory.all, 'pending', 'list'] as const,
    pendingList: (creatorId: string, query?: object) =>
      [...queryKeys.memory.pendingLists(), creatorId, query ?? {}] as const,
    count: (creatorId: string) => [...queryKeys.memory.all, 'pending', 'count', creatorId] as const,
    fragments: (creatorId: string, query?: object) =>
      [...queryKeys.memory.all, 'fragments', creatorId, query ?? {}] as const,
    // V1.81 — whole-Creator SOUL narrative cache (world-agnostic by contract).
    // Distinct from `fragments` so the narrative read/reflect lifecycle
    // invalidates independently of the viz fragments query.
    soulNarrative: (creatorId: string) =>
      [...queryKeys.memory.all, 'soul-narrative', creatorId] as const,
  },
} as const;
