/**
 * Per-creator progressive activation store (plan v1219 § Shared interfaces 1).
 *
 * Local-only unlock state persisted in `localStorage` under
 * `nexus-activation:<creator_id>` as a JSON map `AppId → true`. Only the
 * `brief-saved` event (→ `work`) is ever written this iteration (compass
 * D8/D17/D26); 会话/记忆 have no unlock path and their rail rows carry literal
 * copy instead. shortcut: local-only, not synced across browsers — a
 * service-side store would be a new wire contract (PM decision when a module
 * rebuild requires cross-browser activation).
 */
import { useCallback, useSyncExternalStore } from 'react';

import type { AppId } from '@/lib/app-registry';

const STORAGE_PREFIX = 'nexus-activation:';

/** Unlock event name → app id this iteration. Only `brief-saved` exists (D26). */
const WRITABLE_APP_IDS: Partial<Record<AppId, true>> = { work: true };

const CHANGE_EVENT = 'nexus-activation-change';

let version = 0;

function storageKey(creatorId: string): string {
  return `${STORAGE_PREFIX}${creatorId}`;
}

function readMap(creatorId: string | null): Record<string, true> {
  if (!creatorId || typeof window === 'undefined') return {};
  try {
    const raw = window.localStorage.getItem(storageKey(creatorId));
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    return parsed as Record<string, true>;
  } catch {
    return {};
  }
}

function bumpVersion(): void {
  version += 1;
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event(CHANGE_EVENT));
  }
}

/** True when the creator has unlocked the app locally. Unknown creators/apps → locked. */
export function isUnlocked(creatorId: string | null, appId: AppId): boolean {
  return readMap(creatorId)[appId] === true;
}

/**
 * Persist an unlock for the creator. Only `work` (`brief-saved`) is writable
 * this iteration (D26); any other app id is a deliberate no-op — 会话/记忆
 * ship no unlock path (D17).
 */
export function unlock(creatorId: string, appId: AppId): void {
  if (!WRITABLE_APP_IDS[appId]) return;
  const map = readMap(creatorId);
  if (map[appId] === true) return;
  map[appId] = true;
  try {
    window.localStorage.setItem(storageKey(creatorId), JSON.stringify(map));
  } catch {
    // Storage may be disabled; the unlock is best-effort for this browser.
  }
  bumpVersion();
}

function subscribe(onStoreChange: () => void): () => void {
  const onStorage = (event: StorageEvent) => {
    // Cross-tab writes must bump the snapshot version: useSyncExternalStore
    // re-reads getSnapshot on notify, and an unchanged version bails out
    // without re-rendering (QC S).
    if (event.key?.startsWith(STORAGE_PREFIX)) bumpVersion();
  };
  window.addEventListener(CHANGE_EVENT, onStoreChange);
  window.addEventListener('storage', onStorage);
  return () => {
    window.removeEventListener(CHANGE_EVENT, onStoreChange);
    window.removeEventListener('storage', onStorage);
  };
}

/**
 * React binding for the activation store. Re-renders subscribers when any
 * creator's activation state changes (same-tab unlock or cross-tab storage).
 */
export function useActivation(creatorId: string | null): {
  isUnlocked: (appId: AppId) => boolean;
} {
  // Snapshot is a monotonic version; the predicate re-reads storage so the
  // returned function always sees the latest persisted map.
  useSyncExternalStore(subscribe, () => version, () => version);
  return {
    isUnlocked: useCallback((appId: AppId) => isUnlocked(creatorId, appId), [creatorId, version]),
  };
}
