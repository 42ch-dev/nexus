/**
 * V1.76 B2 — chapter outline content editor save-state machine (`R-V175QC1-S003`).
 *
 * Locks the settle/sync transitions that the V1.75 content-loss fixes depend on
 * (commits 3787c7b3 + 1f0c614c), so the B3 simplification cannot silently
 * regress them. Behavior is asserted via the `SaveStateIndicator` label
 * (Saved / Unsaved changes / Saving…), never implementation details:
 *
 *   - saving -> clean on success (patchIsPending drops, isConflicting false)
 *   - saving -> dirty on 409   (isConflicting true — draft preserved)
 *   - chapterNumber switch     -> clean (content-sync unblocked for new chapter)
 *
 * Faithful orchestrator coupling: the `Harness` owns `patchIsPending` + conflict
 * state and flips pending to TRUE inside the `onPatchChapter` callback — mirroring
 * how `patchChapter.mutate()` makes `patchChapter.isPending` true synchronously
 * in `outline-canvas.tsx`. Without this the settle effect races the save click.
 *
 * The TipTap editor + the `useChapterOutline` read are mocked; `onUpdate` is
 * captured so a test can drive the `clean -> dirty -> saving` path.
 */
import { useState } from 'react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { act, render, screen, fireEvent } from '@testing-library/react';

// Stable outline data so the content-sync effect's `outline.data` dep does not
// churn on every render (keeps the test focused on save-state transitions).
vi.mock('@/api/queries', () => {
  const data = {
    work_id: 'w1',
    chapter: 1,
    volume: 1,
    outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
    content: '# Hello',
    updated_at: '2026-06-29T00:00:00Z',
  };
  return {
    useChapterOutline: () => ({
      data,
      isFetching: false,
      isLoading: false,
      isError: false,
    }),
  };
});

// Minimal TipTap mock. `useEditor` captures the `onUpdate` callback so the test
// can synthesize a user edit (the real path that flips clean -> dirty).
let editorOnUpdate: (() => void) | undefined;
const mockChainRun = vi.fn();
const mockChain = {
  focus: () => mockChain,
  toggleBold: () => mockChain,
  toggleItalic: () => mockChain,
  toggleBulletList: () => mockChain,
  toggleOrderedList: () => mockChain,
  toggleBlockquote: () => mockChain,
  toggleHeading: () => mockChain,
  run: mockChainRun,
};
const mockEditor = {
  isEditable: true,
  setEditable: vi.fn(),
  commands: { setContent: vi.fn() },
  chain: () => mockChain,
  isActive: () => false,
  storage: { markdown: { getMarkdown: () => '# Hello' } },
  destroy: vi.fn(),
};
vi.mock('@tiptap/react', () => ({
  useEditor: (opts: { onUpdate?: () => void }) => {
    editorOnUpdate = opts?.onUpdate;
    return mockEditor;
  },
  EditorContent: () => <div data-testid="editor-surface" />,
}));
vi.mock('@tiptap/starter-kit', () => ({ default: {} }));
vi.mock('tiptap-markdown', () => ({ Markdown: {} }));

import { ChapterOutlineContentEditor } from './chapter-outline-content-editor';

/**
 * Mirrors the outline-canvas orchestrator's coupling: `patchIsPending` flips
 * TRUE synchronously when `onPatchChapter` is invoked (as `patchChapter.isPending`
 * does on `mutate()`), and `settle(withConflict)` resolves the mutation
 * (pending=false, and on a 409 conflict=true).
 */
interface HarnessProps {
  chapterNumber?: number;
  contentVersion?: number;
}
const onPatch = vi.fn();
const controls: { settle: (withConflict: boolean) => void } = { settle: () => {} };

function Harness({ chapterNumber = 1, contentVersion = 0 }: HarnessProps) {
  const [pending, setPending] = useState(false);
  const [conflict, setConflict] = useState(false);
  controls.settle = (withConflict: boolean) => {
    setPending(false);
    if (withConflict) setConflict(true);
  };
  function handlePatch() {
    setPending(true);
    onPatch();
  }
  return (
    <ChapterOutlineContentEditor
      workId="w1"
      chapterNumber={chapterNumber}
      baseRevision={0}
      disabled={false}
      onPatchChapter={handlePatch}
      patchIsPending={pending}
      isConflicting={conflict}
      contentVersion={contentVersion}
    />
  );
}

/** Read the current save-state label shown to the user (the only public
 *  observable of the internal SaveState). */
function saveStateLabel(): string {
  const candidates = ['Saved', 'Unsaved changes', 'Saving…'];
  for (const label of candidates) {
    const node = screen.queryByText(label);
    if (node) return label;
  }
  throw new Error('save-state indicator not rendered');
}

/** Simulate a user edit firing TipTap's onUpdate (clean -> dirty). */
function editContent() {
  act(() => {
    editorOnUpdate?.();
  });
}

/** Simulate the author clicking "Save content" (dirty -> saving + dispatch). */
function clickSave() {
  fireEvent.click(screen.getByRole('button', { name: /Save content/i }));
}

beforeEach(() => {
  editorOnUpdate = undefined;
  onPatch.mockClear();
});

describe('ChapterOutlineContentEditor — save-state transitions (V1.76 B2 / R-V175QC1-S003)', () => {
  it('starts in the Saved (clean) state once the outline loads', () => {
    render(<Harness />);
    expect(saveStateLabel()).toBe('Saved');
  });

  it('transitions saving -> clean when the patch succeeds (patchIsPending drops, no conflict)', () => {
    render(<Harness />);

    // Author edits + saves. The Harness flips patchIsPending true inside the
    // save callback, so 'saving' visibly persists (as it does in the real app).
    editContent();
    expect(saveStateLabel()).toBe('Unsaved changes');
    clickSave();
    expect(saveStateLabel()).toBe('Saving…');
    expect(onPatch).toHaveBeenCalledTimes(1);

    // Orchestrator mutation settles successfully (pending=false, no conflict).
    act(() => controls.settle(false));
    expect(saveStateLabel()).toBe('Saved');
  });

  it('transitions saving -> dirty on a 409 conflict so the draft is preserved', () => {
    render(<Harness />);

    editContent();
    clickSave();
    expect(saveStateLabel()).toBe('Saving…');

    // Mutation settles with the orchestrator's conflict modal open (isConflicting).
    act(() => controls.settle(true));
    // Draft preserved as dirty so the author can re-edit / reapply — NOT reverted
    // to the stale server snapshot while the conflict modal is open.
    expect(saveStateLabel()).toBe('Unsaved changes');
  });

  it('resets to clean on chapterNumber switch (unblocks content-sync for the new chapter)', () => {
    const { rerender } = render(<Harness chapterNumber={1} />);

    // Dirty the editor on chapter 1 (would block a content reload without the
    // chapter-switch reset — the V1.75 3787c7b3 cross-chapter corruption fix).
    editContent();
    expect(saveStateLabel()).toBe('Unsaved changes');

    // Select a different chapter.
    rerender(<Harness chapterNumber={2} />);
    expect(saveStateLabel()).toBe('Saved');
  });
});
