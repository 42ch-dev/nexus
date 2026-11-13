/**
 * Compose-path state for the Strategy canvas (v239-p3-canvas-compose-ui T3/T3b/T4).
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
 * revision, so the user reapplies the op against what the daemon holds.
 */
import { useCallback, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';

import type {
  ListPatternsResponse,
  StrategyComposeRequest,
  StrategyComposeResponse,
  WorkflowDocument,
} from '@42ch/nexus-contracts';

import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';

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

/**
 * Spatial chrome the canvas authors around the committed candidate. Graph
 * structure (nodes, edges, nests) is the daemon's alone.
 */
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

export function useStrategyCompose(presetId: string, enabled: boolean) {
  const client = useNexusClient();

  const patternsQuery = useQuery<ListPatternsResponse>({
    queryKey: ['canvas', 'strategy-compose', 'patterns'],
    queryFn: () => client.listPatterns(),
    enabled,
    staleTime: 30_000,
  });

  // The canonical candidate: the stored row, or the revision-0 import projection
  // for a strategy that has never been composed. Committed compose responses
  // replace it.
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
        // A rejected op leaves the working document unchanged — no auto-retry.
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
        }
        throw error;
      } finally {
        setIsComposing(false);
      }
    },
    [applyResponse, client, presetId],
  );

  /**
   * T4 recovery — reload the canonical candidate at the revision the daemon
   * holds and drop the stale conflict, so the user reapplies the op against
   * current bytes instead of the revision they no longer have.
   */
  const reload = useCallback(async () => {
    const document = await client.getStrategyCandidate(presetId);
    setCommitted({ document, diagnostics: [] });
    setConflict(null);
  }, [client, presetId]);

  /**
   * T3b — nest a pattern under `attachNodeId`. The daemon commits the slot, the
   * region and the connecting edge (`attach_to`), so the committed candidate is
   * what Simulate walks; only the spatial grouping is canvas chrome.
   */
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
        // Nesting into an expanded slot is spatial chrome (the wire records no
        // graph position for a nested region).
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
      const summary = summaries.find(
        (entry) => entry.id === node.pattern?.registry_id && entry.version === node.pattern?.version,
      );
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
