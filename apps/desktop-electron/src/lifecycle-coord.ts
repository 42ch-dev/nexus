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
  // Only a renderer replacement window may attach; normal compatibility→open must dispatch utility open.
  if (
    input.rendererDetached &&
    input.ownerAlive &&
    (input.phase === 'open' || input.phase === 'starting')
  ) {
    return 'attach_existing_owner';
  }
  return 'open_utility';
}

/** Preserve replacement marker across replacement createWindow; clear only on initial bootstrap. */
export function rendererDetachedAfterCreateWindow(input: {
  initialBootstrap: boolean;
  rendererDetached: boolean;
}): boolean {
  return input.initialBootstrap ? false : input.rendererDetached;
}

/** Main-state sequence: renderer crash → replacement createWindow → open policy. */
export function mainStateOpenPolicyAfterRendererCrash(input: {
  phase: LifecyclePhase;
  ownerAlive: boolean;
  initialBootstrap: boolean;
}): {
  rendererDetachedAfterCrash: boolean;
  rendererDetachedAfterCreateWindow: boolean;
  openPolicy: OpenProofPolicy;
} {
  const rendererDetachedAfterCrash = true;
  const markerAfterCreateWindow = rendererDetachedAfterCreateWindow({
    initialBootstrap: input.initialBootstrap,
    rendererDetached: rendererDetachedAfterCrash,
  });
  const openPolicy = resolveOpenProofPolicy({
    reopenRequired: false,
    rendererDetached: markerAfterCreateWindow,
    phase: input.phase,
    ownerAlive: input.ownerAlive,
  });
  return {
    rendererDetachedAfterCrash,
    rendererDetachedAfterCreateWindow: markerAfterCreateWindow,
    openPolicy,
  };
}

/** Readiness returned to a replacement window attaching to an existing owner. */
export function attachExistingReadiness(phase: LifecyclePhase): 'open' | 'starting' {
  return phase === 'open' ? 'open' : 'starting';
}

/** Merge utility-ready snapshot without dropping an in-flight starting phase. */
export function mergeUtilityReadyLifecycle(
  current: { phase: LifecyclePhase; owner_alive: boolean },
  snapshot: { phase?: LifecyclePhase; owner_alive?: boolean } | undefined,
): { phase: LifecyclePhase; owner_alive: boolean } {
  const owner_alive = snapshot?.owner_alive ?? current.owner_alive ?? true;
  if (current.phase === 'starting' && snapshot?.phase !== 'open') {
    return { phase: 'starting', owner_alive };
  }
  return {
    phase: snapshot?.phase ?? current.phase,
    owner_alive,
  };
}

/** Unconfirmed kill must fence reopen until a later confirmed join. */
export function joinExitConfirmed(awaitedExit: boolean): boolean {
  return awaitedExit;
}

/** Whether an unexpected utility exit should interrupt (not suppressed by prior close). */
export function shouldTreatUtilityExitAsUnexpected(
  closeInitiatedForGeneration: number | null,
  ownerGeneration: number,
): boolean {
  return closeInitiatedForGeneration !== ownerGeneration;
}

/** Lifecycle outcome after close request + kill/join — never closed on unconfirmed exit. */
export function resolveCloseLifecycleAfterJoin(input: {
  confirmed: boolean;
  closeOk: boolean;
  ownerStillReferenced: boolean;
  cleanupConfirmed?: boolean | null;
  closeErrorMessage?: string;
}): {
  phase: LifecyclePhase;
  owner_alive: boolean;
  cleanup_confirmed: boolean | null;
  reopenRequired: boolean;
  reason: string | null;
} {
  if (!input.confirmed) {
    return {
      phase: 'interrupted',
      owner_alive: input.ownerStillReferenced,
      cleanup_confirmed: false,
      reopenRequired: true,
      reason: 'utility join unconfirmed during close — owner remains fenced',
    };
  }
  if (input.closeOk) {
    return {
      phase: 'closed',
      owner_alive: false,
      cleanup_confirmed: input.cleanupConfirmed ?? true,
      reopenRequired: false,
      reason: null,
    };
  }
  return {
    phase: 'interrupted',
    owner_alive: false,
    cleanup_confirmed: false,
    reopenRequired: true,
    reason: input.closeErrorMessage ?? 'close failed',
  };
}
