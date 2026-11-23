/**
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
 *
 * One walk runs at a time (a click while one is pending is a no-op), the
 * identity is taken before the request is issued, and a failed read or walk
 * keeps the snapshot in hand: only the error is new.
 */
import { useCallback, useMemo, useRef, useState } from 'react';

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
  isStale: boolean;
  trajectory: SimulationTrajectory | null;
  /** The refused simulate call, kept apart from "never simulated". */
  error: unknown;
  run: (candidate: WorkflowDocument) => Promise<void>;
  runStrategy: (strategyId: string) => Promise<void>;
  reset: () => void;
}

export function useSimulationTrajectory(
  candidate: WorkflowDocument | undefined,
): SimulationState {
  const client = useNexusClient();
  const [snapshot, setSnapshot] = useState<SimulationSnapshot | null>(null);
  const [movedOn, setMovedOn] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [isRunning, setIsRunning] = useState(false);

  const isStale = useMemo(() => {
    if (!snapshot) return false;
    if (!candidate) return movedOn;
    return !sameDocument(snapshot, candidate);
  }, [candidate, snapshot, movedOn]);

  const simulate = useCallback(
    async (doc: WorkflowDocument) => {
      const identity = identityOf(doc);
      const trajectory = await client.simulateStrategy(doc.id, { document: doc });
      setSnapshot({ trajectory, identity });
      setMovedOn(false);
    },
    [client],
  );

  const inFlight = useRef(false);

  const walk = useCallback(
    async (load: () => Promise<WorkflowDocument | null>) => {
      if (inFlight.current) return;
      inFlight.current = true;
      setIsRunning(true);
      setError(null);
      try {
        const doc = await load();
        if (doc) await simulate(doc);
      } catch (caught) {
        setError(caught);
      } finally {
        inFlight.current = false;
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
        if (snapshot && sameDocument(snapshot, doc)) {
          setMovedOn(false);
          return null;
        }
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

  // the snapshot's own staleness (`isStale`) still drives the stale banner.
  // A failed walk outranks staleness too: the error is the newer fact.
  const status: SimulationStatus = isRunning
    ? 'running'
    : error !== null
      ? 'failed'
      : isStale
        ? 'stale'
        : snapshot
          ? 'ready'
          : 'idle';

  return {
    status,
    isStale,
    trajectory: snapshot?.trajectory ?? null,
    error,
    run,
    runStrategy,
    reset,
  };
}
