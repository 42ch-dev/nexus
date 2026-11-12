/**
 * Simulate hook — the trajectory behind the Approve disclosure.
 *
 * `POST /v1/daemon/strategies/{strategy_id}/simulate` is non-mutating but it is
 * a POST, so the result is held as a per-surface snapshot instead of a query:
 * the snapshot records the candidate identity it was produced for, and any
 * later revision/content-hash change makes it stale (Approve is then disabled
 * until the creator re-runs).
 *
 * The client cannot recompute the IR content hash (BLAKE3 canonicalization is
 * Rust-side), so staleness compares the identity fields the candidate carries
 * against the values captured at request time. `ir_content_hash` in the
 * trajectory is the server's recomputation, recorded for display only.
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
  revision: number;
  contentHash: string | null;
}

export type SimulationStatus = 'idle' | 'running' | 'ready' | 'failed' | 'stale';

/** Does the snapshot still describe this document? */
function sameDocument(snapshot: SimulationSnapshot, doc: WorkflowDocument): boolean {
  return (
    doc.revision === snapshot.revision && (doc.content_hash ?? null) === snapshot.contentHash
  );
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
  }, [candidate?.revision, candidate?.content_hash, snapshot, movedOn]);

  const simulate = useCallback(
    async (doc: WorkflowDocument) => {
      const trajectory = await client.simulateStrategy(doc.id, { document: doc });
      setSnapshot({
        trajectory,
        revision: doc.revision,
        contentHash: doc.content_hash ?? null,
      });
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
        // the surface must not read as approval-ready.
        setSnapshot(null);
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
