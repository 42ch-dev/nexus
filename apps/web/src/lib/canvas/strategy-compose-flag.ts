/**
 * `strategy_canvas_compose` flag resolution (compass D14/D18).
 *
 * D18: the flag is scoped to the Strategy Canvas surface only — it never
 * changes Outline, Timeline or any other canvas. D14: with no explicit
 * selection the default is ON for the `developer` entrance and OFF for the
 * `content-creator` entrance; an explicit selection overrides either default.
 *
 * The explicit selection is a per-surface stored value (localStorage in the
 * browser; the desktop IPC entrance seam does not persist flags, so desktop
 * always resolves through the entrance default until a flag store lands).
 */
import { useSyncExternalStore } from 'react';

import { useEntrance } from '@/lib/entrance-context';
import type { EntranceId } from '@/components/layout/entrance-registry';

export const STRATEGY_CANVAS_COMPOSE_FLAG = 'strategy_canvas_compose';

/** Browser persistence key — `nexus-` convention like `nexus-web-locale`. */
export const COMPOSE_FLAG_STORAGE_KEY = 'nexus-flag-strategy-canvas-compose';

export type ComposeFlagSelection = 'on' | 'off';

/** D14 audience default: developer entrance stages compose, content-creator does not. */
export function defaultComposeFlagFor(entrance: EntranceId): boolean {
  return entrance === 'developer';
}

/** D14 resolution: an explicit selection wins over the entrance default. */
export function resolveComposeFlag(
  entrance: EntranceId,
  explicit: ComposeFlagSelection | null,
): boolean {
  if (explicit === 'on') return true;
  if (explicit === 'off') return false;
  return defaultComposeFlagFor(entrance);
}

export function readStoredComposeFlag(): ComposeFlagSelection | null {
  if (typeof window === 'undefined') return null;
  const value = window.localStorage.getItem(COMPOSE_FLAG_STORAGE_KEY);
  return value === 'on' || value === 'off' ? value : null;
}

export function writeStoredComposeFlag(selection: ComposeFlagSelection | null): void {
  if (typeof window === 'undefined') return;
  if (selection === null) {
    window.localStorage.removeItem(COMPOSE_FLAG_STORAGE_KEY);
  } else {
    window.localStorage.setItem(COMPOSE_FLAG_STORAGE_KEY, selection);
  }
}

function subscribe(onStoreChange: () => void): () => void {
  window.addEventListener('storage', onStoreChange);
  return () => window.removeEventListener('storage', onStoreChange);
}

/**
 * The resolved per-surface flag for the Strategy Canvas in this layout tree.
 * Reads the explicit selection through `useSyncExternalStore` so a settings
 * write in another tree (or tab) re-resolves without a reload.
 */
export function useStrategyCanvasComposeFlag(): boolean {
  const { entrance } = useEntrance();
  const explicit = useSyncExternalStore(subscribe, readStoredComposeFlag, () => null);
  return resolveComposeFlag(entrance, explicit);
}
