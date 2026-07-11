/**
 * `useHotkey` — bind a global keyboard shortcut for the lifetime of the calling
 * component (FB-CP-001, V1.111 P0 T2).
 *
 * Architect lock (plan `## Architecture locks` item 3):
 *   `useHotkey(key, handler, opts?: { preventDefault?: boolean })`
 *
 * `key` is a small descriptor:
 *   - `'mod+k'` → fires on ⌘K (mac) / Ctrl+K (win/linux). `mod` is the
 *     cross-platform alias for `metaKey || ctrlKey`, matching the existing
 *     `(e.metaKey || e.ctrlKey)` convention in the strategy canvas
 *     (`use-strategy-canvas.ts`).
 *   - `'k'`     → fires on the bare key with no modifier held.
 *
 * Only a single optional `mod+` prefix is supported; multi-modifiers and key
 * sequences are out of scope (YAGNI — the palette only needs `mod+k`).
 *
 * Conflict-avoidance (HARD, per task brief): the handler is NOT invoked when
 * `document.activeElement` is an `INPUT`, a `TEXTAREA`, or anywhere inside a
 * `[contenteditable]`, `[data-command-palette-ignore]`, or `.react-flow`
 * subtree. ⌘K therefore stays reachable from buttons, links, and the page
 * background, but never steals focus from text entry or the React Flow canvas.
 * This is the contract the command palette (T3) and future surfaces rely on.
 *
 * The handler is stored in a ref so callers may pass an inline closure without
 * re-binding the listener on every render (mirrors the capture-once philosophy
 * of the command registry in `lib/canvas/command-registry.ts`). The listener is
 * bound on `document`, matching `conflict-modal-base.tsx`.
 *
 * Scope note: this is a general primitive, not canvas-scoped — lives directly
 * under `lib/`. P3 owns consolidating the 5 ad-hoc `addEventListener('keydown')`
 * sites onto this hook; this task does NOT refactor them.
 */
import { useEffect, useRef } from 'react';

export interface UseHotkeyOptions {
  /** Call `event.preventDefault()` when the hotkey matches. Default: `true`. */
  readonly preventDefault?: boolean;
}

/**
 * @param key  Descriptor — `'mod+<key>'` (⌘/Ctrl) or a bare `'<key>'`.
 * @param handler Invoked with the matching `KeyboardEvent` when the shortcut
 *                fires. A caller may inspect the event (e.g. read `event.key`);
 *                a caller that ignores it (`() => void`) remains valid. Held in
 *                a ref, so passing a fresh inline closure each render is safe.
 * @param options Optional `{ preventDefault }` (default `true`).
 */
export function useHotkey(
  key: string,
  handler: (event: KeyboardEvent) => void,
  options?: UseHotkeyOptions,
): void {
  const preventDefault = options?.preventDefault ?? true;
  // Capture the latest handler on every render WITHOUT re-binding the listener.
  // An inline closure passed by the caller therefore never goes stale and never
  // triggers an add/remove-listener churn cycle.
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  const { mod, baseKey } = parseDescriptor(key);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent): void {
      if (mod && !(event.metaKey || event.ctrlKey)) return;
      // Bare-key form: ignore when a modifier IS held so `'k'` does not fire on
      // an incidental Ctrl+K (only `mod+k` should match that chord).
      if (!mod && (event.metaKey || event.ctrlKey)) return;
      if (event.key.toLowerCase() !== baseKey) return;
      if (shouldIgnoreTarget(document.activeElement)) return;
      if (preventDefault) event.preventDefault();
      handlerRef.current(event);
    }

    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [mod, baseKey, preventDefault]);
}

/**
 * Parse `'mod+<key>'` or `'<key>'`. Only the `mod+` prefix is recognized; any
 * other `<token>+<key>` form is treated as a literal key name (and will simply
 * fail to match `event.key`, surfacing as a no-op). Documented, not validated
 * — the sole caller (`'mod+k'`) is correct.
 */
function parseDescriptor(descriptor: string): { mod: boolean; baseKey: string } {
  const lower = descriptor.toLowerCase();
  if (lower.startsWith('mod+')) {
    return { mod: true, baseKey: lower.slice('mod+'.length) };
  }
  return { mod: false, baseKey: lower };
}

/**
 * Whether the global hotkey should be suppressed for the currently focused
 * element. `closest()` walks ancestors, so this also covers children of a
 * contenteditable host (inherited editability), children of an opt-out region,
 * and nodes nested inside a React Flow pane.
 */
function shouldIgnoreTarget(element: Element | null): boolean {
  if (element === null) return false;
  const tag = element.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA') return true;
  // Single combined ancestor query — parallel to a CSS selector list, and
  // attribute-based so it is robust across jsdom and real browsers.
  if (element.closest('[contenteditable],[data-command-palette-ignore],.react-flow')) {
    return true;
  }
  return false;
}
