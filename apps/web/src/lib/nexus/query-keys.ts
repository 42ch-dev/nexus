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
  },
  capabilities: {
    all: ['capabilities'] as const,
    list: () => [...queryKeys.capabilities.all, 'list'] as const,
  },
  findings: {
    all: ['findings'] as const,
    list: (workId: string, query?: object) =>
      [...queryKeys.findings.all, 'list', workId, query ?? {}] as const,
  },
  presets: {
    all: ['presets'] as const,
    list: () => [...queryKeys.presets.all, 'list'] as const,
  },
} as const;
