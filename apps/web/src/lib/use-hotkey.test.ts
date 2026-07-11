/**
 * `useHotkey` — global hotkey hook + ⌘K/Ctrl+K conflict-avoidance contract
 * (FB-CP-001, V1.111 P0 T2).
 *
 * Coverage mirrors the task brief: fires on ⌘K/Ctrl+K; ignored when an INPUT,
 * TEXTAREA, contenteditable host, `.react-flow` pane, or
 * `[data-command-palette-ignore]` region is focused; still fires from a button
 * and the page background; `preventDefault` behavior; latest-handler ref
 * (no stale closure, no re-bind churn); cleanup on unmount.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';

import { useHotkey } from './use-hotkey';

/** Dispatch a cancelable `keydown` on `document` with the given key + modifiers. */
function dispatchKey(key: string, opts: { meta?: boolean; ctrl?: boolean } = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key,
    metaKey: opts.meta ?? false,
    ctrlKey: opts.ctrl ?? false,
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

/** Append a focusable child inside a container; return cleanup that removes both. */
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

describe('useHotkey — mod+k (⌘K / Ctrl+K)', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('fires the handler on ⌘K (metaKey)', () => {
    const handler = vi.fn();
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('k', { meta: true });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('fires the handler on Ctrl+K (ctrlKey)', () => {
    const handler = vi.fn();
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('k', { ctrl: true });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('does not fire on bare k (no modifier held)', () => {
    const handler = vi.fn();
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('k');
    expect(handler).not.toHaveBeenCalled();
  });

  it('does not fire for unrelated keys even with mod', () => {
    const handler = vi.fn();
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('s', { meta: true });
    expect(handler).not.toHaveBeenCalled();
  });
});

describe('useHotkey — handler receives the KeyboardEvent (architect lock)', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('passes the matching KeyboardEvent to the handler', () => {
    const handler = vi.fn();
    renderHook(() => useHotkey('mod+k', handler));
    const event = dispatchKey('k', { meta: true });
    expect(handler).toHaveBeenCalledTimes(1);
    // Architect lock item 3: `handler: (e: KeyboardEvent) => void`. The event
    // is forwarded so a caller may inspect it; a `() => void` caller remains
    // valid (a function ignoring its argument is assignable).
    expect(handler).toHaveBeenCalledWith(event);
    expect(handler.mock.calls[0][0]).toBeInstanceOf(KeyboardEvent);
  });

  it('accepts a `() => void` handler (ignores the event argument)', () => {
    const handler = vi.fn();
    renderHook(() => useHotkey('mod+k', () => handler()));
    dispatchKey('k', { meta: true });
    expect(handler).toHaveBeenCalledTimes(1);
  });
});

describe('useHotkey — preventDefault', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('calls preventDefault by default when the hotkey matches', () => {
    const handler = vi.fn();
    renderHook(() => useHotkey('mod+k', handler));
    const event = new KeyboardEvent('keydown', {
      key: 'k',
      metaKey: true,
      bubbles: true,
      cancelable: true,
    });
    const spy = vi.spyOn(event, 'preventDefault');
    document.dispatchEvent(event);
    expect(spy).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('does not call preventDefault when opts.preventDefault is false', () => {
    const handler = vi.fn();
    renderHook(() => useHotkey('mod+k', handler, { preventDefault: false }));
    const event = new KeyboardEvent('keydown', {
      key: 'k',
      metaKey: true,
      bubbles: true,
      cancelable: true,
    });
    const spy = vi.spyOn(event, 'preventDefault');
    document.dispatchEvent(event);
    expect(spy).not.toHaveBeenCalled();
    expect(handler).toHaveBeenCalledTimes(1);
  });
});

describe('useHotkey — conflict-avoidance (ignored targets)', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('is ignored when an INPUT is focused', () => {
    const handler = vi.fn();
    const cleanup = focusElement('input');
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('k', { meta: true });
    expect(handler).not.toHaveBeenCalled();
    cleanup();
  });

  it('is ignored when a TEXTAREA is focused', () => {
    const handler = vi.fn();
    const cleanup = focusElement('textarea');
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('k', { meta: true });
    expect(handler).not.toHaveBeenCalled();
    cleanup();
  });

  it('is ignored when a contenteditable host is focused', () => {
    const handler = vi.fn();
    const cleanup = focusElement('div', { contenteditable: 'true' });
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('k', { meta: true });
    expect(handler).not.toHaveBeenCalled();
    cleanup();
  });

  it('is ignored when a focusable child inside a [contenteditable] host is focused', () => {
    // The host itself is tested above; here a `<button>` (a focusable
    // descendant) sits inside the contenteditable subtree. `closest()` ancestor
    // matching must still suppress activation.
    const handler = vi.fn();
    const cleanup = focusInsideContainer('div', { contenteditable: 'true' }, 'button');
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('k', { meta: true });
    expect(handler).not.toHaveBeenCalled();
    cleanup();
  });

  it('is ignored when focused inside a .react-flow pane', () => {
    const handler = vi.fn();
    const cleanup = focusInsideContainer('div', { class: 'react-flow' });
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('k', { meta: true });
    expect(handler).not.toHaveBeenCalled();
    cleanup();
  });

  it('is ignored when focused inside a [data-command-palette-ignore] region', () => {
    const handler = vi.fn();
    const cleanup = focusInsideContainer('div', { 'data-command-palette-ignore': '' });
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('k', { meta: true });
    expect(handler).not.toHaveBeenCalled();
    cleanup();
  });

  it('still fires when a BUTTON is focused (globally reachable)', () => {
    const handler = vi.fn();
    const cleanup = focusElement('button');
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('k', { meta: true });
    expect(handler).toHaveBeenCalledTimes(1);
    cleanup();
  });

  it('still fires when the page background (body) is focused', () => {
    const handler = vi.fn();
    renderHook(() => useHotkey('mod+k', handler));
    dispatchKey('k', { meta: true });
    expect(handler).toHaveBeenCalledTimes(1);
  });
});

describe('useHotkey — binding lifecycle', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('removes the listener on unmount (cleanup contract)', () => {
    const handler = vi.fn();
    const { unmount } = renderHook(() => useHotkey('mod+k', handler));
    unmount();
    dispatchKey('k', { meta: true });
    expect(handler).not.toHaveBeenCalled();
  });

  it('uses the latest handler via ref (no stale closure, no re-bind churn)', () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = renderHook(({ h }) => useHotkey('mod+k', h), {
      initialProps: { h: first },
    });
    rerender({ h: second });
    dispatchKey('k', { meta: true });
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });
});
