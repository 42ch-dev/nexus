import type { LifecyclePhase } from './ipc.js';

/** Milliseconds remaining on an absolute close deadline (never negative). */
export function remainingCloseBudget(deadlineAt: number, now = Date.now()): number {
  return Math.max(0, deadlineAt - now);
}

/** Generation to attach pending IPC work after ensuring the utility owner exists. */
export function pendingGenerationAfterEnsure(
  _generationBeforeEnsure: number,
  generationAfterEnsure: number,
): number {
  return generationAfterEnsure;
}

export type OpenProofPolicy = 'reopen_after_fence' | 'attach_existing_owner' | 'open_utility';

export function resolveOpenProofPolicy(input: {
  reopenRequired: boolean;
  rendererDetached: boolean;
  phase: LifecyclePhase;
  ownerAlive: boolean;
}): OpenProofPolicy {
  if (input.reopenRequired) {
    return 'reopen_after_fence';
  }
  if (
    input.ownerAlive &&
    (input.phase === 'open' || input.phase === 'starting') &&
    (input.rendererDetached || input.phase === 'open')
  ) {
    return 'attach_existing_owner';
  }
  return 'open_utility';
}

/** Unconfirmed kill must fence reopen until a later confirmed join. */
export function joinExitConfirmed(awaitedExit: boolean): boolean {
  return awaitedExit;
}
