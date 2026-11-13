/**
 * The single YAML-authority write boundary for the Strategy canvas.
 *
 * The legacy surface has many structural writers — header create, palette
 * command, spatial connect, keyboard create, edge inspector save, draft
 * conflict replay. They all dispatch through {@link createStrategyYamlAuthority},
 * so when the compose candidate owns the document every op in
 * {@link STRUCTURAL_WRITE_OPS} is refused here instead of at each control.
 * Prompt templates are content, not structure, and never enter this boundary.
 */
import { useMemo, useRef } from 'react';
import type {
  StrategyPatchResponse,
  StrategyPatchStateRequest,
  StrategyPatchTransitionRequest,
} from '@42ch/nexus-contracts';

import { useNexusClient } from '@/lib/client-context';
import type { NexusClient } from '@/lib/nexus';

/** Every structural write the legacy YAML canvas can dispatch. */
export const STRUCTURAL_WRITE_OPS = ['state.patch', 'transition.create', 'transition.update'] as const;

export type StructuralWriteOp = (typeof STRUCTURAL_WRITE_OPS)[number];

/** Typed refusal for a structural YAML write while the candidate owns the document. */
export class StructuralWriteRefusedError extends Error {
  readonly code = 'structural_write_refused';

  constructor(readonly op: StructuralWriteOp) {
    super(`Structural write "${op}" is refused: the compose candidate owns the document.`);
    this.name = 'StructuralWriteRefusedError';
  }
}

type StrategyYamlWriter = Pick<NexusClient, 'strategyPatchState' | 'strategyPatchTransition'>;

export interface StrategyYamlAuthority {
  patchState(
    strategyId: string,
    stateId: string,
    request: StrategyPatchStateRequest,
  ): Promise<StrategyPatchResponse>;
  patchTransition(
    strategyId: string,
    request: StrategyPatchTransitionRequest,
  ): Promise<StrategyPatchResponse>;
}

export function createStrategyYamlAuthority(
  writer: StrategyYamlWriter,
  candidateIsAuthority: () => boolean,
): StrategyYamlAuthority {
  async function dispatch<T>(op: StructuralWriteOp, write: () => Promise<T>): Promise<T> {
    if (candidateIsAuthority()) throw new StructuralWriteRefusedError(op);
    return write();
  }

  return {
    patchState: (strategyId, stateId, request) =>
      dispatch('state.patch', () => writer.strategyPatchState(strategyId, stateId, request)),
    patchTransition: (strategyId, request) =>
      // `op` is the wire contract's only structural discriminator (create | update).
      dispatch(request.op === 'create' ? 'transition.create' : 'transition.update', () =>
        writer.strategyPatchTransition(strategyId, request),
      ),
  };
}

/**
 * Live read of the flag: a conflict replay that crosses a flag flip is still
 * refused, because the authority reads the current value at dispatch time.
 */
export function useStrategyYamlAuthority(structuralWritesDisabled: boolean): StrategyYamlAuthority {
  const client = useNexusClient();
  const disabledRef = useRef(structuralWritesDisabled);
  disabledRef.current = structuralWritesDisabled;
  return useMemo(
    () => createStrategyYamlAuthority(client, () => disabledRef.current),
    [client],
  );
}
