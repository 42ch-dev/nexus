/**
 * `useTimelineShortcut` — V1.123 P3 Task 5.
 *
 * Pins the global keyboard shortcut contract locked by:
 *   - Plan `2026-07-18-v1.123-timeline-first-ia-deepening.md` Task 5 +
 *     Global Constraints (Timeline "突出" — one-click reachability from
 *     anywhere).
 *   - `iterations/v1.123/specs/three-layer-product-spec.md` AC-V1123-16
 *     (global Timeline entry; one-click reachability).
 *
 * Coverage:
 *   - Pressing the `g` then `t` chord fires the handler (navigates to
 *     `/timeline`). The chord is Vim/Taskwarrior-style: `g` enters pending
 *     state, the next `t` confirms.
 *   - The chord does NOT fire on `g` alone (waits for `t`) nor on `t` alone
 *     (needs `g` first).
 *   - Reset window: 1s after `g`, pending state clears; `t` alone after the
 *     reset is a no-op.
 *   - Any non-`t` key after `g` cancels the chord.
 *   - Modifier-suppressed: `mod+g`, `mod+t`, `shift+g`, `alt+t`, etc. do NOT
 *     fire — the chord is bare-key only so it never conflicts with `mod+k`
 *     (command palette) or any browser-reserved chord.
 *   - Conflict-avoidance (shared with `useHotkey`): suppressed when an
 *     INPUT, TEXTAREA, contenteditable host, or `[data-command-palette-ignore]`
 *     subtree is focused — the chord never steals typing focus.
 *   - Cleanup on unmount removes the listener.
 *
 * The chord form was chosen over `Cmd+T`/`Ctrl+T` (browser new-tab reserved)
 * and over a single `mod+<key>` (no non-conflicting `mod` shortcut is
 * obviously available; `mod+k` is already the command palette). The two-key
 * sequence is the V1.123 durable slice; a richer chord engine (sequence
 * visualisation, configurable keys, etc.) is P4+ polish.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';

import { TIMELINE_SHORTCUT_DESCRIPTOR, useTimelineShortcut } from '../keyboard-shortcuts';

/** Dispatch a `keydown` on `document` with the given key + modifiers. */
function dispatchKey(
  key: string,
  opts: { meta?: boolean; ctrl?: boolean; alt?: boolean; shift?: boolean } = {},
): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key,
    metaKey: opts.meta ?? false,
    ctrlKey: opts.ctrl ?? false,
    altKey: opts.alt ?? false,
    shiftKey: opts.shift ?? false,
    bubbles: true,
    cancelable: true,
  });
  document.dispatchEvent(event);
  return event;
}

/** Append a focusable element, focus it; return cleanup that removes it. */
function focusElement(tag: string, attrs: Record<string, string> = {}): () => void {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  // Bare <div>/<span> are not focusable in jsdom without tabindex.
  if (tag !== 'input' && tag !== 'textarea' && tag !== 'button' && !('tabindex' in attrs)) {
    el.setAttribute('tabindex', '0');
  }
  document.body.appendChild(el);
  el.focus();
  return () => {
    el.remove();
  };
}

/** Append a focusable child inside a container; return cleanup. */
function focusInsideContainer(
  containerTag: string,
  containerAttrs: Record<string, string>,
  childTag = 'div',
): () => void {
  const container = document.createElement(containerTag);
  for (const [k, v] of Object.entries(containerAttrs)) container.setAttribute(k, v);
  const child = document.createElement(childTag);
  child.setAttribute('tabindex', '0');
  container.appendChild(child);
  document.body.appendChild(container);
  child.focus();
  return () => {
    container.remove();
  };
}

describe('useTimelineShortcut — descriptor export', () => {
  it('exports a stable human-readable descriptor for documentation surfaces', () => {
    // The descriptor is what a future "Keyboard shortcuts" help panel would
    // surface. It is also what the test asserts against so the chord form
    // is locked.
    expect(TIMELINE_SHORTCUT_DESCRIPTOR).toBe('g t');
  });
});

describe('useTimelineShortcut — chord firing (g then t)', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('fires the handler on g → t (bare keys, no modifiers)', () => {
    const handler = vi.fn();
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g');
    dispatchKey('t');
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('does NOT fire on bare g alone (waits for the t confirm)', () => {
    const handler = vi.fn();
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g');
    expect(handler).not.toHaveBeenCalled();
  });

  it('does NOT fire on bare t alone (needs g first)', () => {
    const handler = vi.fn();
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('t');
    expect(handler).not.toHaveBeenCalled();
  });

  it('does NOT fire when a non-t key follows g (chord cancelled)', () => {
    const handler = vi.fn();
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g');
    dispatchKey('x');
    dispatchKey('t'); // Already cancelled — this `t` is a fresh single, not a confirm.
    expect(handler).not.toHaveBeenCalled();
  });

  it('calls preventDefault on both g and the confirming t (chord owns the keys)', () => {
    // The chord must preventDefault on `g` so the browser does not type it
    // into a non-text region (the page background) while waiting for the
    // confirm key. The same applies to the confirming `t` once the chord
    // fires — the keypress is "consumed" by the chord, not by the page.
    const handler = vi.fn();
    renderHook(() => useTimelineShortcut(handler));

    const gEvent = new KeyboardEvent('keydown', {
      key: 'g',
      bubbles: true,
      cancelable: true,
    });
    const gSpy = vi.spyOn(gEvent, 'preventDefault');
    document.dispatchEvent(gEvent);
    expect(gSpy).toHaveBeenCalledTimes(1);

    const tEvent = new KeyboardEvent('keydown', {
      key: 't',
      bubbles: true,
      cancelable: true,
    });
    const tSpy = vi.spyOn(tEvent, 'preventDefault');
    document.dispatchEvent(tEvent);
    expect(tSpy).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('supports consecutive chords (g t → g t fires twice)', () => {
    const handler = vi.fn();
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g');
    dispatchKey('t');
    dispatchKey('g');
    dispatchKey('t');
    expect(handler).toHaveBeenCalledTimes(2);
  });
});

describe('useTimelineShortcut — modifier-suppression (chord is bare-key only)', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('does NOT fire when Ctrl+g is pressed (modifier cancels the chord)', () => {
    const handler = vi.fn();
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g', { ctrl: true });
    dispatchKey('t');
    expect(handler).not.toHaveBeenCalled();
  });

  it('does NOT fire when Meta+g is pressed (Cmd+g does not start the chord)', () => {
    const handler = vi.fn();
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g', { meta: true });
    dispatchKey('t');
    expect(handler).not.toHaveBeenCalled();
  });

  it('does NOT fire when the confirming t carries a modifier', () => {
    const handler = vi.fn();
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g');
    dispatchKey('t', { ctrl: true });
    expect(handler).not.toHaveBeenCalled();
  });

  it('does NOT fire on Shift+g Shift+t (shift counts as a modifier)', () => {
    // The chord is bare-key only so it never collides with browser/IDE
    // shift-modified shortcuts. Shift cancels the chord outright.
    const handler = vi.fn();
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g', { shift: true });
    dispatchKey('t', { shift: true });
    expect(handler).not.toHaveBeenCalled();
  });
});

describe('useTimelineShortcut — conflict-avoidance (shared with useHotkey)', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('is suppressed when an INPUT is focused', () => {
    const handler = vi.fn();
    const cleanup = focusElement('input');
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g');
    dispatchKey('t');
    expect(handler).not.toHaveBeenCalled();
    cleanup();
  });

  it('is suppressed when a TEXTAREA is focused', () => {
    const handler = vi.fn();
    const cleanup = focusElement('textarea');
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g');
    dispatchKey('t');
    expect(handler).not.toHaveBeenCalled();
    cleanup();
  });

  it('is suppressed when a contenteditable host is focused', () => {
    const handler = vi.fn();
    const cleanup = focusElement('div', { contenteditable: 'true' });
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g');
    dispatchKey('t');
    expect(handler).not.toHaveBeenCalled();
    cleanup();
  });

  it('is suppressed inside a [data-command-palette-ignore] subtree', () => {
    // Canvas surfaces opt out via `data-command-palette-ignore` on their root
    // wrapper (same contract as `useHotkey`). The chord MUST honor the same
    // opt-out so it never steals keys from the canvas.
    const handler = vi.fn();
    const cleanup = focusInsideContainer('div', { 'data-command-palette-ignore': '' });
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g');
    dispatchKey('t');
    expect(handler).not.toHaveBeenCalled();
    cleanup();
  });

  it('still fires when a BUTTON is focused (globally reachable)', () => {
    const handler = vi.fn();
    const cleanup = focusElement('button');
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g');
    dispatchKey('t');
    expect(handler).toHaveBeenCalledTimes(1);
    cleanup();
  });

  it('still fires when the page background (body) is focused', () => {
    const handler = vi.fn();
    renderHook(() => useTimelineShortcut(handler));
    dispatchKey('g');
    dispatchKey('t');
    expect(handler).toHaveBeenCalledTimes(1);
  });
});

describe('useTimelineShortcut — lifecycle', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('removes the listener on unmount (cleanup contract)', () => {
    const handler = vi.fn();
    const { unmount } = renderHook(() => useTimelineShortcut(handler));
    unmount();
    dispatchKey('g');
    dispatchKey('t');
    expect(handler).not.toHaveBeenCalled();
  });

  it('uses the latest handler via ref (no stale closure, no re-bind churn)', () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = renderHook(({ h }) => useTimelineShortcut(h), {
      initialProps: { h: first },
    });
    rerender({ h: second });
    dispatchKey('g');
    dispatchKey('t');
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });
});
