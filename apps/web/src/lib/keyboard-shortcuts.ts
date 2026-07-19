/**
 * `useTimelineShortcut` — bind the global Timeline navigation shortcut for
 * the lifetime of the calling component (V1.123 P3 Task 5).
 *
 * Pins the V1.123 IA-deepening contract: Timeline is the central instrument,
 * so it MUST be reachable from anywhere via a single chord (AC-V1123-16 +
 * plan `2026-07-18-v1.123-timeline-first-ia-deepening.md` Task 5).
 *
 * Chord form: `g` then `t` (bare keys, no modifiers).
 *   - `g` enters pending state (1s window). `preventDefault` keeps the
 *     browser from typing into the page background while the chord waits.
 *   - `t` within the window fires the handler (the orchestrator's
 *     `useNavigate('/timeline')` callback).
 *   - Any other key after `g` cancels the chord.
 *   - Any modifier held (`Ctrl`/`Cmd`/`Alt`/`Shift`) cancels the chord so
 *     the shortcut never collides with `mod+k` (command palette),
 *     `Cmd+T`/`Ctrl+T` (browser new-tab), or any browser/IDE reserved chord.
 *
 * Conflict-avoidance (shared contract with `useHotkey` — V1.111 P0 T2):
 * suppressed when `document.activeElement` is an INPUT, a TEXTAREA, or
 * anywhere inside a `[contenteditable]` or `[data-command-palette-ignore]`
 * subtree. The chord therefore never steals typing focus nor fires inside
 * the canvas (which opts out via `data-command-palette-ignore`).
 *
 * Design rationale — why a chord and not `mod+<key>`:
 *   - `mod+t` is browser-reserved (new tab).
 *   - `mod+k` is the command palette; `mod+s` is browser save; `mod+9`
 *     et al. conflict with tab-switching.
 *   - The plan brief suggested "Cmd+T or G T chord" — the chord is the
 *     non-conflicting choice. The `useHotkey` primitive does not support
 *     two-key sequences, so this hook owns its listener.
 *
 * `simplify:` two-key chord state machine owned at this call site. A
 * future iteration MAY consolidate chord handling into `useHotkey` itself
 * (descriptor form `'g>t'` or similar); until then, the chord surface is
 * the only one in the app, and a small dedicated hook is the durable
 * V1.123 slice.
 */
import { useEffect, useRef } from 'react';

import { isHotkeyTargetIgnored } from './use-hotkey';

/**
 * Human-readable descriptor for documentation surfaces (e.g. a future
 * "Keyboard shortcuts" help panel). Locked at `'g t'` so any UI surfacing
 * the shortcut reads the same string the test asserts against.
 */
export const TIMELINE_SHORTCUT_DESCRIPTOR = 'g t';

/**
 * Pending-chord reset window. After `g` is pressed, the hook waits this long
 * for the confirming `t`. Tuned for a comfortable human double-tap (~750 ms
 * is the median inter-key interval for two-key chords per HCI literature;
 * 1s adds a safety margin).
 *
 * `simplify:` constant — not configurable in V1.123. If users find the
 * window too short / long, exposing it via a setting is a one-line change.
 */
const CHORD_RESET_MS = 1000;

/**
 * @param handler Invoked when the `g → t` chord completes. Held in a ref so
 *                callers may pass an inline closure without re-binding the
 *                listener on every render (mirrors `useHotkey`).
 */
export function useTimelineShortcut(handler: () => void): void {
  // Capture the latest handler on every render WITHOUT re-binding the
  // listener. An inline closure passed by the caller therefore never goes
  // stale and never triggers an add/remove-listener churn cycle.
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let pending = false;
    let resetTimer: ReturnType<typeof setTimeout> | null = null;

    function reset(): void {
      pending = false;
      if (resetTimer !== null) {
        clearTimeout(resetTimer);
        resetTimer = null;
      }
    }

    function onKeyDown(event: KeyboardEvent): void {
      // Suppression contract shared with `useHotkey` — never steal keys from
      // text entry / contenteditable / canvas opt-out regions.
      if (isHotkeyTargetIgnored(document.activeElement)) {
        // If the user was mid-chord and then focused a text field, cancel
        // the chord so a subsequent `t` typing in the field does not fire
        // the shortcut on refocus.
        reset();
        return;
      }

      // Any modifier held → chord cancelled. Bare-key only so the shortcut
      // never collides with `mod+k` (palette), Cmd/Ctrl+T (new tab), or any
      // browser-reserved chord.
      if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) {
        reset();
        return;
      }

      const key = event.key.toLowerCase();
      if (key === 'g') {
        // Start the chord (or restart if pressed twice in a row — `g g t`
        // does not fire; only the most-recent `g` carries forward).
        reset();
        pending = true;
        resetTimer = setTimeout(reset, CHORD_RESET_MS);
        // preventDefault keeps the browser from typing `g` into the page
        // background while the chord waits for the confirm key. The chord
        // owns the keystroke.
        event.preventDefault();
        return;
      }

      if (pending && key === 't') {
        reset();
        event.preventDefault();
        handlerRef.current();
        return;
      }

      // Any other key after `g` cancels the chord.
      reset();
    }

    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      reset();
    };
  }, []);
}
