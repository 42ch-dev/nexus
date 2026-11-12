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

export interface SimulationState {
  status: SimulationStatus;
  trajectory: SimulationTrajectory | null;
  /** The refused simulate call, kept apart from "never simulated". */
  error: unknown;
  run: (candidate: WorkflowDocument) => Promise<void>;
  reset: () => void;
}

export function useSimulationTrajectory(
  candidate: WorkflowDocument | undefined,
): SimulationState {
  const client = useNexusClient();
  const [snapshot, setSnapshot] = useState<SimulationSnapshot | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [isRunning, setIsRunning] = useState(false);

  const isStale = useMemo(() => {
    if (!snapshot) return false;
    return (
      candidate?.revision !== snapshot.revision ||
      (candidate?.content_hash ?? null) !== snapshot.contentHash
    );
  }, [candidate?.revision, candidate?.content_hash, snapshot]);

  const run = useCallback(
    async (doc: WorkflowDocument) => {
      setIsRunning(true);
      setError(null);
      try {
        const trajectory = await client.simulateStrategy(doc.id, { document: doc });
        setSnapshot({
          trajectory,
          revision: doc.revision,
          contentHash: doc.content_hash ?? null,
        });
      } catch (caught) {
        // A refused simulate is an explicit failure, never an empty trajectory:
        // the surface must not read as approval-ready.
        setSnapshot(null);
        setError(caught);
      } finally {
        setIsRunning(false);
      }
    },
    [client],
  );

  const reset = useCallback(() => {
    setSnapshot(null);
    setError(null);
  }, []);

  const status: SimulationStatus = isRunning
    ? 'running'
    : error !== null
      ? 'failed'
      : snapshot
        ? isStale
          ? 'stale'
          : 'ready'
        : 'idle';

  return { status, trajectory: snapshot?.trajectory ?? null, error, run, reset };
}
