/**
 *
 * The canvas holds the candidate document in hand: seeded from the canonical
 * candidate read (`GET /strategies/{id}/candidate`, the stored row or the
 * revision-0 import projection) and replaced by every committed compose
 * response, which is the authoritative candidate at `new_revision`. The
 * last-read revision is the CAS guard every op sends as `base_revision`.
 *
 * T3b (I-000423): `nest` writes the connecting edge itself (`attach_to`), so
 * the slot is reachable on the committed document and Simulate walks it. The
 * canvas authors no edge: the only chrome it keeps is spatial (grouping and
 * expand/collapse), never graph structure.
 *
 * T4: a refused op (409/422) leaves the working document unchanged and is never
 * auto-retried. The 409 recovery reloads the canonical candidate at its current
 * revision, so the user reapplies the op against what the daemon holds. Any
 * other failure (a 500, a network drop) surfaces as an error toast — callers
 * still consume the rejection, so nothing fails silently.
 */
import { useCallback, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import type {
  ListPatternsResponse,
  StrategyComposeRequest,
  StrategyComposeResponse,
  WorkflowDocument,
} from '@42ch/nexus-contracts';

import { useNexusClient } from '@/lib/client-context';
import { NexusClientError, type NexusClient } from '@/lib/nexus';
import { useToast } from '@/lib/use-toast';

export interface ComposeConflict {
  /** The daemon's message: "Candidate changed before this edit. Reload revision N and reapply the change." */
  message: string;
  baseRevision: number;
  currentRevision: number;
}

export interface ComposeRefusal {
  /** Named refusal family (`nest_cycle`, `package_unavailable`, the slot refusals, …). */
  reason: string;
  message: string;
  details: Record<string, unknown>;
}

export interface UnboundSlot {
  slotNodeId: string;
  patternId: string;
  patternVersion: string;
  slots: string[];
}

export interface NestTarget {
  /** Existing node the new slot hangs off; the interaction wires the connecting edge. */
  attachNodeId: string;
  package: { id: string; version: string };
}

interface CommittedCandidate {
  document: WorkflowDocument;
  diagnostics: StrategyComposeResponse['validation_summary'];
}

interface ComposeChrome {
  containment: Record<string, string>;
  expandedSlots: ReadonlySet<string>;
}

function dropChromeFor(
  chrome: ComposeChrome,
  removedNodeIds: ReadonlySet<string>,
): ComposeChrome {
  return {
    containment: Object.fromEntries(
      Object.entries(chrome.containment).filter(
        ([child, parent]) => !removedNodeIds.has(child) && !removedNodeIds.has(parent),
      ),
    ),
    expandedSlots: new Set([...chrome.expandedSlots].filter((id) => !removedNodeIds.has(id))),
  };
}

/** `nest` needs fresh ids; suffix until the id is free in the candidate. */
function freshId(base: string, taken: (id: string) => boolean): string {
  let id = base;
  let n = 2;
  while (taken(id)) {
    id = `${base}-${n}`;
    n += 1;
  }
  return id;
}

/**
 *
 * shortcut: 20 pages is the whole registry for any plausible package count;
 * upgrade: raise it if a real registry outgrows 20 listing pages.
 */
const MAX_PATTERN_PAGES = 20;

async function listAllPatterns(client: NexusClient): Promise<ListPatternsResponse> {
  const items: ListPatternsResponse['items'] = [];
  let cursor: string | undefined;
  for (let page = 0; page < MAX_PATTERN_PAGES; page += 1) {
    const response = await client.listPatterns(cursor === undefined ? undefined : { cursor });
    items.push(...response.items);
    const next = response.pagination.next_cursor;
    if (!response.pagination.has_more || next === undefined || next === cursor) {
      return { items, pagination: { limit: response.pagination.limit, has_more: false } };
    }
    cursor = next;
  }
  throw new Error(
    `The pattern listing did not finish within ${MAX_PATTERN_PAGES} pages.`,
  );
}

export function useStrategyCompose(presetId: string, enabled: boolean) {
  const client = useNexusClient();
  const { toast } = useToast();
  const { t } = useTranslation('common');

  const patternsQuery = useQuery<ListPatternsResponse>({
    queryKey: ['canvas', 'strategy-compose', 'patterns'],
    queryFn: () => listAllPatterns(client),
    enabled,
    staleTime: 30_000,
  });

  const seedQuery = useQuery({
    queryKey: ['canvas', 'strategy-compose', 'seed', presetId],
    queryFn: () => client.getStrategyCandidate(presetId),
    enabled,
    staleTime: 30_000,
  });

  const [committed, setCommitted] = useState<CommittedCandidate | null>(null);
  const [chrome, setChrome] = useState<ComposeChrome>({
    containment: {},
    expandedSlots: new Set(),
  });
  const [conflict, setConflict] = useState<ComposeConflict | null>(null);
  const [refusal, setRefusal] = useState<ComposeRefusal | null>(null);
  const [isComposing, setIsComposing] = useState(false);

  const committedDocument = committed?.document ?? seedQuery.data ?? null;

  /** The candidate the daemon holds, verbatim: the canvas adds no graph structure. */
  const workingDocument = committedDocument;

  const revision = committedDocument?.revision ?? 0;
  const diagnostics = committed?.diagnostics ?? [];

  const applyResponse = useCallback(
    (response: StrategyComposeResponse) => {
      setCommitted({
        document: response.document,
        diagnostics: response.validation_summary,
      });
      setChrome((prev) => {
        const nodeIds = new Set(response.document.nodes.map((node) => node.id));
        const removed = new Set(
          [
            ...Object.keys(prev.containment),
            ...Object.values(prev.containment),
            ...prev.expandedSlots,
          ].filter((id) => !nodeIds.has(id)),
        );
        return removed.size > 0 ? dropChromeFor(prev, removed) : prev;
      });
    },
    [],
  );

  const runCompose = useCallback(
    async (request: StrategyComposeRequest) => {
      setIsComposing(true);
      try {
        const response = await client.composeStrategy(presetId, request);
        applyResponse(response);
        setRefusal(null);
        return response;
      } catch (error) {
        if (error instanceof NexusClientError && error.code === 'strategy_candidate_conflict') {
          const details = (error.details ?? {}) as { current_revision?: number };
          setConflict({
            message: error.message,
            baseRevision: request.base_revision,
            currentRevision: details.current_revision ?? 0,
          });
        } else if (error instanceof NexusClientError && error.status === 422) {
          const details = (error.details ?? {}) as Record<string, unknown>;
          setRefusal({
            reason: typeof details.reason === 'string' ? details.reason : 'compose_refused',
            message: error.message,
            details,
          });
        } else {
          toast({
            variant: 'error',
            title: t('error.couldNotComposeStrategyEdit'),
            description: error instanceof Error ? error.message : t('error.unexpected'),
          });
        }
        throw error;
      } finally {
        setIsComposing(false);
      }
    },
    [applyResponse, client, presetId, t, toast],
  );

  const reload = useCallback(async () => {
    try {
      const document = await client.getStrategyCandidate(presetId);
      setCommitted((prev) =>
        prev && prev.document.revision > document.revision
          ? prev
          : { document, diagnostics: [] },
      );
      setConflict(null);
      setRefusal(null);
    } catch (error) {
      setRefusal({
        reason: 'reload_failed',
        message: error instanceof Error ? error.message : 'Reload failed.',
        details: {},
      });
    }
  }, [client, presetId]);

  const nestPattern = useCallback(
    async ({ attachNodeId, package: pkg }: NestTarget) => {
      if (!committedDocument) return;
      const slotNodeId = freshId(
        `${pkg.id}-slot`,
        (id) => committedDocument.nodes.some((node) => node.id === id),
      );
      const nestId = freshId(pkg.id, (id) =>
        committedDocument.nests.some((nest) => nest.id === id),
      );
      const attachToSlot =
        committedDocument.nodes.find((node) => node.id === attachNodeId)?.kind === 'pattern_slot';
      await runCompose({
        base_revision: committedDocument.revision,
        op: 'nest',
        payload: {
          nest_id: nestId,
          parent_node: slotNodeId,
          attach_to: attachNodeId,
          package: { id: pkg.id, version: pkg.version },
          expansion: 'expand_in_candidate',
        },
      });
      setChrome((prev) => ({
        ...prev,
        containment: attachToSlot
          ? { ...prev.containment, [slotNodeId]: attachNodeId }
          : prev.containment,
        expandedSlots: new Set([...prev.expandedSlots, slotNodeId]),
      }));
    },
    [committedDocument, runCompose],
  );

  const unnest = useCallback(
    async (nestId: string) => {
      if (!committedDocument) return;
      await runCompose({
        base_revision: committedDocument.revision,
        op: 'unnest',
        payload: { nest_id: nestId },
      });
    },
    [committedDocument, runCompose],
  );

  const replaceSubtree = useCallback(
    async (nestId: string, pkg: { id: string; version: string }) => {
      if (!committedDocument) return;
      await runCompose({
        base_revision: committedDocument.revision,
        op: 'replace_subtree',
        payload: { nest_id: nestId, package: pkg, expansion: 'expand_in_candidate' },
      });
    },
    [committedDocument, runCompose],
  );

  const bindSlot = useCallback(
    async (nestId: string, slot: string, value: unknown) => {
      if (!committedDocument) return;
      await runCompose({
        base_revision: committedDocument.revision,
        op: 'bind_slot',
        payload: { nest_id: nestId, slot, value },
      });
    },
    [committedDocument, runCompose],
  );

  const toggleSlotExpanded = useCallback((slotNodeId: string) => {
    setChrome((prev) => ({
      ...prev,
      expandedSlots: prev.expandedSlots.has(slotNodeId)
        ? new Set([...prev.expandedSlots].filter((id) => id !== slotNodeId))
        : new Set([...prev.expandedSlots, slotNodeId]),
    }));
  }, []);

  /** Required slots with no binding — the approve gate names them per pattern. */
  const unboundSlots = useMemo<UnboundSlot[]>(() => {
    if (!workingDocument) return [];
    const summaries = patternsQuery.data?.items ?? [];
    const result: UnboundSlot[] = [];
    for (const node of workingDocument.nodes) {
      if (node.kind !== 'pattern_slot') continue;
      const pinned = (entry: ListPatternsResponse['items'][number]) =>
        entry.id === node.pattern?.registry_id && entry.version === node.pattern?.version;
      const summary =
        summaries.find((entry) => entry.available && pinned(entry)) ??
        summaries.find((entry) => pinned(entry));
      const unbound = (summary?.slots ?? [])
        .filter((slot) => slot.required && node.bind?.[slot.name] === undefined)
        .map((slot) => slot.name);
      if (unbound.length > 0 && node.pattern) {
        result.push({
          slotNodeId: node.id,
          patternId: node.pattern.registry_id,
          patternVersion: node.pattern.version,
          slots: unbound,
        });
      }
    }
    return result;
  }, [workingDocument, patternsQuery.data]);

  return {
    patternsQuery,
    seedQuery,
    workingDocument,
    revision,
    diagnostics,
    unboundSlots,
    containment: chrome.containment,
    expandedSlots: chrome.expandedSlots,
    conflict,
    refusal,
    isComposing,
    isLoading: enabled && (seedQuery.isLoading || patternsQuery.isLoading),
    nestPattern,
    unnest,
    replaceSubtree,
    bindSlot,
    toggleSlotExpanded,
    reload,
    dismissConflict: useCallback(() => setConflict(null), []),
    dismissRefusal: useCallback(() => setRefusal(null), []),
  };
}
