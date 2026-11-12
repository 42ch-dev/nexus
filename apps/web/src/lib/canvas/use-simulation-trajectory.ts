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
  /**
   * Whether the held trajectory no longer describes the document. Read apart
   * from `status` so the stale banner survives a walk in flight (`running`) or
   * a failed re-read (`failed`).
   */
  isStale: boolean;
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
      // Taken before the request: a mutation while the POST is in flight must
      // not record an identity the server never walked.
      const identity = identityOf(doc);
      const trajectory = await client.simulateStrategy(doc.id, { document: doc });
      setSnapshot({ trajectory, identity });
      setMovedOn(false);
    },
    [client],
  );

  const inFlight = useRef(false);

  const walk = useCallback(
    // A `null` load result means the held trajectory still describes the
    // document, so nothing is walked.
    async (load: () => Promise<WorkflowDocument | null>) => {
      // One walk at a time. `status` cannot carry this alone: a stale snapshot
      // is re-walked while it is still held, so a second click would otherwise
      // start a second walk.
      if (inFlight.current) return;
      inFlight.current = true;
      setIsRunning(true);
      setError(null);
      try {
        const doc = await load();
        if (doc) await simulate(doc);
      } catch (caught) {
        // A refused read/walk must not discard a trajectory in hand: the
        // snapshot still records the document the server walked, so keep it.
        // Only the error is new, and the next open re-reads.
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
          // The document came back to what the held trajectory walked, so the
          // moved-on mark no longer applies and must not pin the surface stale.
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

  // A walk in flight outranks a stale snapshot so the surface shows progress;
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
