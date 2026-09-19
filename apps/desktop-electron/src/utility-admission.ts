/**
 * Lifecycle admission for the app-managed service owner (v1.192 P0-T4).
 *
 * The utility process owns at most one service handle and answers at most one
 * lifecycle request at a time, so every operation is admitted against the
 * current owner state before it can touch the service or the native binding.
 * An unconfirmed close retains the owner and refuses work that would race it.
 */

import type { UtilityOperation } from './service-controller.js';

export type ServiceOwnerState = 'none' | 'running' | 'closing' | 'closed' | 'unconfirmed';

export type UtilityAdmission = { ok: true } | { ok: false; code: string; message: string };

const RETRY_FIRST = 'the previous close was not confirmed; the retained owner must be released first';

export function admitUtilityOperation(
  operation: UtilityOperation,
  state: ServiceOwnerState,
): UtilityAdmission {
  if (state === 'unconfirmed' && operation !== 'close') {
    return { ok: false, code: 'interrupted', message: RETRY_FIRST };
  }
  switch (operation) {
    case 'start': {
      if (state === 'running') {
        return { ok: false, code: 'owner_busy', message: 'a service is already running in this owner' };
      }
      if (state === 'closing') {
        return { ok: false, code: 'busy', message: 'a service close is still in flight' };
      }
      return { ok: true };
    }
    case 'close':
      // Idempotent join and retry path: a confirmed close is final, an
      // unconfirmed one may be retried until cleanup is confirmed.
      return { ok: true };
    case 'reset-local-state': {
      if (state === 'running' || state === 'closing') {
        return {
          ok: false,
          code: 'busy',
          message: 'local-state reset requires the service to be closed first',
        };
      }
      return { ok: true };
    }
  }
}
