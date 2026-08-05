/**
 * Centralized TanStack Query keys for the Nexus Daemon API resources.
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
  // V1.94 — Creator profile switcher + agent scan.
  creators: {
    all: ['creators'] as const,
    list: (query?: object) => [...queryKeys.creators.all, 'list', query ?? {}] as const,
    active: () => [...queryKeys.creators.all, 'active'] as const,
  },
  agentHost: {
    all: ['agentHost'] as const,
    scan: (request?: { filter?: string; registry_refresh?: boolean }) =>
      [...queryKeys.agentHost.all, 'scan', request?.filter ?? 'all', request?.registry_refresh ?? false] as const,
  },
  // V1.120 P1 (T1) — desktop-only saved agent profile. Cached so the Settings
  // Agent Save handler can invalidate it and the DaemonStatusBar badge refreshes
  // immediately after a save (AD-P1-1) instead of waiting for the 10s poll.
  agentProfile: {
    all: ['agentProfile'] as const,
    detail: () => [...queryKeys.agentProfile.all, 'detail'] as const,
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
    // V1.82 — workspace-scoped world list for the SOUL selector.
    worlds: () => [...queryKeys.memory.all, 'worlds'] as const,
    // V1.81 → V1.82: whole-Creator or per-World SOUL narrative cache. The query
    // key includes the selected `world_id` so switching scopes creates exactly
    // one active observer per active scope and never leaves a stale narrative visible.
    soulNarrative: (creatorId: string, worldId?: string | null) =>
      [...queryKeys.memory.all, 'soul-narrative', creatorId, worldId ?? 'creator'] as const,
  },
  // V1.89 — Deeper Manuscript Reading (BL-11 MVP slice).
  reading: {
    all: ['reading'] as const,
    progress: (workId: string, chapter: number) =>
      [...queryKeys.reading.all, 'progress', workId, chapter] as const,
    annotations: (workId: string, chapter: number) =>
      [...queryKeys.reading.all, 'annotations', workId, chapter] as const,
  },
  // V1.114 — Compute modules registry visibility.
  compute: {
    all: ['compute'] as const,
    modules: {
      all: () => [...queryKeys.compute.all, 'modules'] as const,
      list: () => [...queryKeys.compute.modules.all(), 'list'] as const,
      detail: (moduleId: string) =>
        [...queryKeys.compute.modules.all(), 'detail', moduleId] as const,
    },
    // V1.147 P1 — Run Studio history. `lists()` covers every filter variant so
    // run/accept/discard mutations invalidate all visible runs lists at once.
    runs: {
      all: () => [...queryKeys.compute.all, 'runs'] as const,
      lists: () => [...queryKeys.compute.runs.all(), 'list'] as const,
      list: (filter?: object) => [...queryKeys.compute.runs.lists(), filter ?? {}] as const,
      details: () => [...queryKeys.compute.runs.all(), 'detail'] as const,
      detail: (runId: string) => [...queryKeys.compute.runs.details(), runId] as const,
    },
  },
  // V1.151 P1 — Assembly Inspector (DF-76). Read-only moment packet; no
  // mutation invalidates it (the UI observes the route, AC-I6).
  inspector: {
    all: ['inspector'] as const,
    moment: (request?: object) =>
      [...queryKeys.inspector.all, 'moment', request ?? {}] as const,
  },
  timeline: {
    all: ['timeline'] as const,
    overview: (cursor?: string) =>
      [...queryKeys.timeline.all, 'overview', cursor ?? '__first'] as const,
    // V1.147 P2 — per-World timeline log events (machine-written families,
    // e.g. compute_result). `all()` prefix-covers every world + filter so
    // accept/discard invalidation via `timeline.all` refetches the mounted
    // canvas events query.
    events: {
      all: () => [...queryKeys.timeline.all, 'events'] as const,
      list: (worldId: string, filter?: object) =>
        [...queryKeys.timeline.events.all(), worldId, filter ?? {}] as const,
    },
  },
} as const;
