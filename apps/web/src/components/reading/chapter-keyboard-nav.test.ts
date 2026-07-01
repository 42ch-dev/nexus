/**
 * Chapter keyboard-nav interaction tests (R-V179P0-QC1-001).
 *
 * Interaction-level coverage for the ←/→ chapter/volume navigation hook +
 * the overlay/editable short-circuit guards. The hook owns a window `keydown`
 * listener, so these tests mount it via `renderHook`, fire real `keydown`
 * events (RTL `fireEvent.keyDown` on `document.body`), and assert the injected
 * `navigate` spy received the expected chapter route — or was never called
 * when a guard short-circuits.
 *
 * Pure predicate coverage for {@link hasOpenOverlay} / {@link isEditable}
 * (DOM-shape based) lives alongside, so the role-pattern assumption
 * (`[role=menu]:not([hidden])` / `[role=dialog]:not([hidden])`) is locked by a
 * regression test.
 */
import { act, fireEvent, renderHook, cleanup } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  hasOpenOverlay,
  isEditable,
  useChapterKeyboardNav,
  type ChapterKeyboardNeighbors,
} from '@/components/reading/chapter-keyboard-nav';

function keyDown(key: string, init: Partial<KeyboardEventInit> = {}) {
  fireEvent.keyDown(document.body, { key, bubbles: true, ...init });
}

const PREV = { chapter: 2, volume: 1 };
const NEXT = { chapter: 4, volume: 3 };
const NEIGHBORS: ChapterKeyboardNeighbors = { prev: PREV, next: NEXT };

describe('useChapterKeyboardNav — ←/→ navigation', () => {
  afterEach(() => {
    cleanup();
    document.body.innerHTML = '';
  });

  it('navigates to the previous chapter on ArrowLeft', () => {
    const navigate = vi.fn();
    renderHook(() => useChapterKeyboardNav('work-1', NEIGHBORS, navigate));

    act(() => keyDown('ArrowLeft'));

    expect(navigate).toHaveBeenCalledOnce();
    expect(navigate).toHaveBeenCalledWith('/works/work-1/chapters/2?volume=1');
  });

  it('navigates to the next chapter on ArrowRight and forwards the volume', () => {
    const navigate = vi.fn();
    renderHook(() => useChapterKeyboardNav('work-1', NEIGHBORS, navigate));

    act(() => keyDown('ArrowRight'));

    expect(navigate).toHaveBeenCalledWith('/works/work-1/chapters/4?volume=3');
  });

  it('does nothing when there is no neighbor in the pressed direction', () => {
    const navigate = vi.fn();
    renderHook(() =>
      useChapterKeyboardNav('work-1', { prev: null, next: NEXT }, navigate),
    );

    act(() => keyDown('ArrowLeft'));
    expect(navigate).not.toHaveBeenCalled();

    act(() => keyDown('ArrowRight'));
    expect(navigate).toHaveBeenCalledOnce();
  });

  it('ignores modifier-key variants so app/cmd+arrow shortcuts are unaffected', () => {
    const navigate = vi.fn();
    renderHook(() => useChapterKeyboardNav('work-1', NEIGHBORS, navigate));

    act(() => keyDown('ArrowLeft', { metaKey: true }));
    act(() => keyDown('ArrowRight', { ctrlKey: true }));
    act(() => keyDown('ArrowLeft', { altKey: true }));

    expect(navigate).not.toHaveBeenCalled();
  });

  it('ignores already-prevented events', () => {
    const navigate = vi.fn();
    // jsdom resets `defaultPrevented` when an event is constructed+prevented
    // before dispatch, so simulate a real earlier listener that cancels the
    // event: registered before the hook's listener (both bubble-phase on
    // window), it runs first and the hook observes `defaultPrevented === true`.
    const cancel = (e: KeyboardEvent) => e.preventDefault();
    window.addEventListener('keydown', cancel);
    renderHook(() => useChapterKeyboardNav('work-1', NEIGHBORS, navigate));
    try {
      act(() => keyDown('ArrowLeft'));
    } finally {
      window.removeEventListener('keydown', cancel);
    }

    expect(navigate).not.toHaveBeenCalled();
  });

  it('URL-encodes the work id', () => {
    const navigate = vi.fn();
    renderHook(() => useChapterKeyboardNav('work/1', NEIGHBORS, navigate));

    act(() => keyDown('ArrowRight'));

    expect(navigate).toHaveBeenCalledWith('/works/work%2F1/chapters/4?volume=3');
  });
});

describe('useChapterKeyboardNav — guard short-circuits', () => {
  afterEach(() => {
    cleanup();
    document.body.innerHTML = '';
    (document.activeElement as HTMLElement | null)?.blur?.();
  });

  it('does not navigate when a [role="dialog"] is open', () => {
    const navigate = vi.fn();
    renderHook(() => useChapterKeyboardNav('work-1', NEIGHBORS, navigate));

    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    document.body.appendChild(dialog);

    act(() => keyDown('ArrowLeft'));

    expect(navigate).not.toHaveBeenCalled();
  });

  it('does not navigate when a [role="menu"] is open', () => {
    const navigate = vi.fn();
    renderHook(() => useChapterKeyboardNav('work-1', NEIGHBORS, navigate));

    const menu = document.createElement('div');
    menu.setAttribute('role', 'menu');
    document.body.appendChild(menu);

    act(() => keyDown('ArrowRight'));

    expect(navigate).not.toHaveBeenCalled();
  });

  it('navigates again once the overlay is removed', () => {
    const navigate = vi.fn();
    renderHook(() => useChapterKeyboardNav('work-1', NEIGHBORS, navigate));

    const menu = document.createElement('div');
    menu.setAttribute('role', 'menu');
    document.body.appendChild(menu);
    act(() => keyDown('ArrowRight'));
    expect(navigate).not.toHaveBeenCalled();

    menu.remove();
    act(() => keyDown('ArrowRight'));
    expect(navigate).toHaveBeenCalledOnce();
  });

  it('treats a [hidden] overlay as closed (does navigate)', () => {
    const navigate = vi.fn();
    renderHook(() => useChapterKeyboardNav('work-1', NEIGHBORS, navigate));

    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    dialog.setAttribute('hidden', '');
    document.body.appendChild(dialog);

    act(() => keyDown('ArrowLeft'));

    expect(navigate).toHaveBeenCalledOnce();
  });

  it('does not navigate while an input is focused', () => {
    const navigate = vi.fn();
    renderHook(() => useChapterKeyboardNav('work-1', NEIGHBORS, navigate));

    const input = document.createElement('input');
    document.body.appendChild(input);
    input.focus();

    act(() => keyDown('ArrowLeft'));

    expect(navigate).not.toHaveBeenCalled();
  });

  it('does not navigate while a textarea is focused', () => {
    const navigate = vi.fn();
    renderHook(() => useChapterKeyboardNav('work-1', NEIGHBORS, navigate));

    const ta = document.createElement('textarea');
    document.body.appendChild(ta);
    ta.focus();

    act(() => keyDown('ArrowRight'));

    expect(navigate).not.toHaveBeenCalled();
  });

  it('does not navigate inside a contenteditable element', () => {
    const navigate = vi.fn();
    renderHook(() => useChapterKeyboardNav('work-1', NEIGHBORS, navigate));

    const ce = document.createElement('div');
    // `contenteditable` makes the div focusable (so document.activeElement
    // becomes it); jsdom never computes isContentEditable, so stub the property
    // the production predicate reads to exercise the guard.
    ce.setAttribute('contenteditable', 'true');
    Object.defineProperty(ce, 'isContentEditable', { configurable: true, value: true });
    document.body.appendChild(ce);
    ce.focus();

    act(() => keyDown('ArrowLeft'));

    expect(navigate).not.toHaveBeenCalled();
  });
});

describe('hasOpenOverlay — role-pattern predicate', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('is false when no overlay is present', () => {
    expect(hasOpenOverlay()).toBe(false);
  });

  it('is true for a visible [role="menu"]', () => {
    const menu = document.createElement('div');
    menu.setAttribute('role', 'menu');
    document.body.appendChild(menu);
    expect(hasOpenOverlay()).toBe(true);
  });

  it('is true for a visible [role="dialog"]', () => {
    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    document.body.appendChild(dialog);
    expect(hasOpenOverlay()).toBe(true);
  });

  it('is false for a [hidden] overlay', () => {
    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    dialog.setAttribute('hidden', '');
    document.body.appendChild(dialog);
    expect(hasOpenOverlay()).toBe(false);
  });
});

describe('isEditable — focus predicate', () => {
  it('flags input/textarea/select elements', () => {
    for (const tag of ['INPUT', 'TEXTAREA', 'SELECT']) {
      const el = document.createElement(tag);
      expect(isEditable(el)).toBe(true);
    }
  });

  it('flags contenteditable elements', () => {
    const el = document.createElement('div');
    // jsdom does not compute isContentEditable from the attribute; stub the
    // property the production predicate reads.
    Object.defineProperty(el, 'isContentEditable', { configurable: true, value: true });
    expect(isEditable(el)).toBe(true);
  });

  it('does not flag a plain div', () => {
    const el = document.createElement('div');
    expect(isEditable(el)).toBe(false);
  });
});
