/**
 * V1.75 A8 — chapter outline content editor accessibility contract.
 *
 * Locks the WCAG 2.1 AA surface of the in-inspector TipTap toolbar so a future
 * refactor cannot silently drop the toggle semantics, labels, or keyboard
 * reachability that the V1.65 editor parity requires.
 *
 * The TipTap editor + the `useChapterOutline` read are mocked; this test
 * asserts the ARIA/keyboard contract of the toolbar chrome, not ProseMirror
 * behavior (covered by the round-trip test).
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';

// Mock the outline read so the editor mounts without a daemon round-trip.
vi.mock('@/api/queries', () => {
  return {
    useChapterOutline: () => {
      return {
        data: {
          work_id: 'w1',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Hello',
          updated_at: '2026-06-29T00:00:00Z',
        },
        isFetching: false,
        isLoading: false,
        isError: false,
      };
    },
  };
});

// Minimal TipTap mock: the toolbar reads `editor.isActive(...)` + drives toggles
// through `editor.chain().focus().<toggle>().run()`. Provide just enough surface.
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

vi.mock('@tiptap/react', () => {
  return {
    useEditor: () => mockEditor,
    EditorContent: () => <div data-testid="editor-surface" />,
  };
});
vi.mock('@tiptap/starter-kit', () => ({ default: {} }));
vi.mock('tiptap-markdown', () => ({ Markdown: {} }));

import { ChapterOutlineContentEditor } from './chapter-outline-content-editor';

function renderEditor(disabled = false) {
  return render(
    <ChapterOutlineContentEditor
      workId="w1"
      chapterNumber={1}
      baseRevision={0}
      disabled={disabled}
      onPatchChapter={vi.fn()}
      patchIsPending={false}
      contentVersion={0}
    />,
  );
}

beforeEach(() => {
  mockChainRun.mockClear();
});

describe('ChapterOutlineContentEditor — WCAG 2.1 AA toolbar contract (V1.75 A8)', () => {
  it('renders a toolbar region with an accessible label', () => {
    renderEditor();
    expect(
      screen.getByRole('toolbar', { name: /Outline content formatting/i }),
    ).toBeInTheDocument();
  });

  it('exposes every formatting toggle as a labeled, aria-pressed button', () => {
    renderEditor();
    const labels = [
      'Heading 1',
      'Heading 2',
      'Heading 3',
      'Bold',
      'Italic',
      'Bullet list',
      'Numbered list',
      'Quote',
    ];
    for (const label of labels) {
      const btn = screen.getByRole('button', { name: label });
      expect(btn).toBeInTheDocument();
      // aria-pressed must be present so screen readers announce toggle state.
      expect(btn).toHaveAttribute('aria-pressed');
    }
  });

  it('keeps the editor region labeled for assistive tech', () => {
    renderEditor();
    expect(
      screen.getByRole('region', { name: /Chapter 1 outline content editor/i }),
    ).toBeInTheDocument();
  });

  it('disables the toolbar toggles when the chapter is read-only', () => {
    renderEditor(true);
    const bold = screen.getByRole('button', { name: 'Bold' });
    expect(bold).toBeDisabled();
  });
});
