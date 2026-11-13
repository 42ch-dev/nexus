/**
 * Compose-path state for the Strategy canvas (v239-p3-canvas-compose-ui T3/T3b/T4).
 *
 * The canvas holds the candidate document in hand: seeded from the one IR read
 * the wire offers (`GET /strategies/{id}/workflow-ir`, the import projection at
 * revision 0) and replaced by every committed compose response, which is the
 * authoritative candidate at `new_revision`. The last-read revision is the CAS
 * guard every op sends as `base_revision`.
 *
 * T3b (I-000423): the compose wire has no edge-carrying op and `nest` writes no
 * edge, so the connecting edge that makes a nested slot reachable — the walk
 * only visits nodes an edge reaches — is authored by the nest interaction and
 * kept as canvas chrome on top of the committed document. Simulate walks the
 * working document and the overlay names the slot.
 * shortcut: the edge is not persisted — the stored candidate trails the working
 * document by exactly that edge until the compose wire grows an edge op, so an
 * approve of a nested candidate is refused `simulation_stale` by the daemon.
 *
 * T4: a refused op (409/422) leaves the working document unchanged and is never
 * auto-retried. The 409 recovery (reload the canonical candidate and reapply)
 * is blocked on the missing candidate read route — `reloadAvailable` stays
 * false instead of inventing a client-side cache (R1, compose-ops report).
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

type IrEdge = WorkflowDocument['edges'][number];

export interface ComposeConflict {
  /** The daemon's message: "Candidate changed before this edit. Reload revision N and reapply the change." */
  message: string;
  baseRevision: number;
  currentRevision: number;
  /**
   * False while no candidate read route exists: recovery cannot reload the
   * canonical candidate, so the dialog names the gap instead of offering a
   * reload that would clobber the candidate with the import projection.
   */
  reloadAvailable: false;
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

/** Chrome the canvas authors around the committed candidate. */
interface ComposeChrome {
  edges: IrEdge[];
  containment: Record<string, string>;
  expandedSlots: ReadonlySet<string>;
}

function dropChromeFor(
  chrome: ComposeChrome,
  removedNodeIds: ReadonlySet<string>,
): ComposeChrome {
  return {
    edges: chrome.edges.filter(
      (edge) => !removedNodeIds.has(edge.from) && !removedNodeIds.has(edge.to),
    ),
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

  // The only candidate-shaped read the wire serves today. It projects the
  // installed preset (revision 0); committed compose responses replace it.
  const seedQuery = useQuery({
    queryKey: ['canvas', 'strategy-compose', 'seed', presetId],
    queryFn: () => client.getStrategyWorkflowIr(presetId),
    enabled,
    staleTime: 30_000,
  });

  const [committed, setCommitted] = useState<CommittedCandidate | null>(null);
  const [chrome, setChrome] = useState<ComposeChrome>({
    edges: [],
    containment: {},
    expandedSlots: new Set(),
  });
  const [conflict, setConflict] = useState<ComposeConflict | null>(null);
  const [refusal, setRefusal] = useState<ComposeRefusal | null>(null);
  const [isComposing, setIsComposing] = useState(false);

  const committedDocument = committed?.document ?? seedQuery.data ?? null;

  /** Committed candidate plus the chrome the canvas authored around it. */
  const workingDocument = useMemo<WorkflowDocument | null>(() => {
    if (!committedDocument) return null;
    if (chrome.edges.length === 0) return committedDocument;
    const edgeIds = new Set(committedDocument.edges.map((edge) => edge.id));
    const extra = chrome.edges.filter((edge) => !edgeIds.has(edge.id));
    if (extra.length === 0) return committedDocument;
    return { ...committedDocument, edges: [...committedDocument.edges, ...extra] };
  }, [committedDocument, chrome.edges]);

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
            ...prev.edges.flatMap((edge) => [edge.from, edge.to]),
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
            reloadAvailable: false,
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
   * T3b — nest a pattern and wire the connecting edge. The daemon commits the
   * slot node + region; the walk only visits the slot through an edge, so the
   * interaction authors `attach → slot` as canvas chrome (see module doc).
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
      await runCompose({
        base_revision: committedDocument.revision,
        op: 'nest',
        payload: {
          nest_id: nestId,
          parent_node: slotNodeId,
          package: { id: pkg.id, version: pkg.version },
          expansion: 'expand_in_candidate',
        },
      });
      setChrome((prev) => ({
        ...prev,
        edges: [
          ...prev.edges,
          { id: `authored-${attachNodeId}-${slotNodeId}`, from: attachNodeId, to: slotNodeId, kind: 'linear' },
        ],
        // Nesting into an expanded slot is spatial chrome (the wire records no
        // graph position for a nested region).
        containment:
          committedDocument.nodes.find((node) => node.id === attachNodeId)?.kind === 'pattern_slot'
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
    dismissConflict: useCallback(() => setConflict(null), []),
    dismissRefusal: useCallback(() => setRefusal(null), []),
  };
}
