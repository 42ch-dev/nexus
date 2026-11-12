/**
 * Simulate hook — the trajectory behind the Approve disclosure.
 *
 * `POST /v1/daemon/strategies/{strategy_id}/simulate` is non-mutating but it is
 * a POST, so the result is held as a per-surface snapshot instead of a query:
 * the snapshot records the identity of the exact document it was walked from,
 * and a later content change makes it stale (Approve is then disabled until
 * the creator re-runs). `ir_content_hash` in the trajectory is the server's
 * recomputation, recorded for display only.
 *
 * The server's `revision` and `content_hash` cannot carry that identity: the
 * IR route emits them as constants, so two fetches of a changed document look
 * identical. The client compares the document itself instead, canonically
 * serialised with sorted object keys (the server's canonical-JSON spirit).
 *
 * A fetched (installed-preset) document has no live candidate prop, so
 * `runStrategy` re-reads it and compares the same identity itself: unchanged
 * keeps the trajectory in hand, moved marks it stale and walks it again.
 */
import { useCallback, useMemo, useState } from 'react';

import type { SimulationTrajectory, WorkflowDocument } from '@42ch/nexus-contracts';

import { useNexusClient } from '@/lib/client-context';

export interface SimulationSnapshot {
  trajectory: SimulationTrajectory;
  /** Canonical identity of the document the trajectory was walked from. */
  identity: string;
}

export type SimulationStatus = 'idle' | 'running' | 'ready' | 'failed' | 'stale';

/** Canonical JSON: object keys sorted, so equal documents serialise identically. */
function canonicalJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>)
      .filter(([, entry]) => entry !== undefined)
      .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0));
    return `{${entries
      .map(([key, entry]) => `${JSON.stringify(key)}:${canonicalJson(entry)}`)
      .join(',')}}`;
  }
  return JSON.stringify(value);
}

/**
 * Identity of a document's content. `revision` and `content_hash` are left out
 * on purpose: they are the fields the route hardcodes.
 */
function identityOf(doc: WorkflowDocument): string {
  const { revision: _revision, content_hash: _contentHash, ...content } = doc;
  return canonicalJson(content);
}

/** Does the snapshot still describe this document? */
function sameDocument(snapshot: SimulationSnapshot, doc: WorkflowDocument): boolean {
  return identityOf(doc) === snapshot.identity;
}

export interface SimulationState {
  status: SimulationStatus;
  trajectory: SimulationTrajectory | null;
  /** The refused simulate call, kept apart from "never simulated". */
  error: unknown;
  run: (candidate: WorkflowDocument) => Promise<void>;
  /**
   * Simulate the installed strategy: re-read its candidate IR and walk it only
   * when that IR no longer matches the trajectory in hand.
   */
  runStrategy: (strategyId: string) => Promise<void>;
  reset: () => void;
}

export function useSimulationTrajectory(
  candidate: WorkflowDocument | undefined,
): SimulationState {
  const client = useNexusClient();
  const [snapshot, setSnapshot] = useState<SimulationSnapshot | null>(null);
  // Set when a re-read installed document moved on: the held trajectory is
  // superseded even though no candidate prop exists to compare against.
  const [movedOn, setMovedOn] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [isRunning, setIsRunning] = useState(false);

  const isStale = useMemo(() => {
    if (!snapshot) return false;
    // A fetched document has no live candidate prop; `runStrategy` decides.
    if (!candidate) return movedOn;
    return !sameDocument(snapshot, candidate);
  }, [candidate, snapshot, movedOn]);

  const simulate = useCallback(
    async (doc: WorkflowDocument) => {
      const trajectory = await client.simulateStrategy(doc.id, { document: doc });
      setSnapshot({ trajectory, identity: identityOf(doc) });
      setMovedOn(false);
    },
    [client],
  );

  const walk = useCallback(
    // A `null` load result means the held trajectory still describes the
    // document, so nothing is walked.
    async (load: () => Promise<WorkflowDocument | null>) => {
      setIsRunning(true);
      setError(null);
      try {
        const doc = await load();
        if (doc) await simulate(doc);
      } catch (caught) {
        // A refused read/walk is an explicit failure, never an empty trajectory:
        // the surface must not read as approval-ready. A stale flag from the
        // failed attempt would outrank the error, so it goes too.
        setSnapshot(null);
        setMovedOn(false);
        setError(caught);
      } finally {
        setIsRunning(false);
      }
    },
    [simulate],
  );

  const run = useCallback(
    (doc: WorkflowDocument) => walk(() => Promise.resolve(doc)),
    [walk],
  );

  const runStrategy = useCallback(
    (strategyId: string) =>
      walk(async () => {
        const doc = await client.getStrategyWorkflowIr(strategyId);
        if (snapshot && sameDocument(snapshot, doc)) return null;
        if (snapshot) setMovedOn(true);
        return doc;
      }),
    [client, walk, snapshot],
  );

  const reset = useCallback(() => {
    setSnapshot(null);
    setMovedOn(false);
    setError(null);
  }, []);

  // A stale trajectory is named as such even while its re-walk is in flight:
  // the surface must never present a superseded trajectory as current.
  const status: SimulationStatus = isStale
    ? 'stale'
    : isRunning
      ? 'running'
      : error !== null
        ? 'failed'
        : snapshot
          ? 'ready'
          : 'idle';

  return { status, trajectory: snapshot?.trajectory ?? null, error, run, runStrategy, reset };
}
