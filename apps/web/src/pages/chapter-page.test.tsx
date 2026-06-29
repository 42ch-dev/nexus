/**
 * ChapterPage render tests — outline editor load/save/dirty state, soft-
 * concurrency warning banner, body read-only render, and copy-path context menu.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { Route, Routes } from 'react-router-dom';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { ChapterPage } from '@/pages/chapter-page';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

// TipTap relies on DOM layout APIs jsdom does not implement (getClientRects).
// Mock the editor surface so tests can assert load/save/dirty state without
// exercising ProseMirror layout.
let capturedOnUpdate: (() => void) | undefined;
let currentMarkdown = '# Chapter 1';

const mockEditor = {
  isEditable: true,
  setEditable: vi.fn((next: boolean) => {
    mockEditor.isEditable = next;
  }),
  commands: {
    setContent: vi.fn((content: string) => {
      currentMarkdown = content;
    }),
  },
  chain: () => ({
    focus: () => ({
      toggleBold: () => ({ run: vi.fn() }),
      toggleItalic: () => ({ run: vi.fn() }),
      toggleBulletList: () => ({ run: vi.fn() }),
      toggleOrderedList: () => ({ run: vi.fn() }),
      toggleBlockquote: () => ({ run: vi.fn() }),
      toggleHeading: () => ({ run: vi.fn() }),
    }),
  }),
  isActive: () => false,
  storage: { markdown: { getMarkdown: vi.fn(() => currentMarkdown) } },
};

vi.mock('@tiptap/react', () => ({
  useEditor: vi.fn((options: { onUpdate?: () => void; content?: string }) => {
    capturedOnUpdate = options.onUpdate;
    if (options.content !== undefined) currentMarkdown = options.content;
    return mockEditor;
  }),
  EditorContent: ({ editor }: { editor?: typeof mockEditor }) => {
    const content = editor?.storage.markdown.getMarkdown() ?? '';
    return (
      <textarea
        defaultValue={content}
        aria-label="Outline editor"
        onChange={(e) => {
          currentMarkdown = e.target.value;
          capturedOnUpdate?.();
        }}
      />
    );
  },
}));

vi.mock('@tiptap/starter-kit', () => ({ default: {} }));
vi.mock('tiptap-markdown', () => ({ Markdown: {} }));

const client = () => new BrowserClient();

beforeEach(() => {
  capturedOnUpdate = undefined;
  currentMarkdown = '# Chapter 1';
  mockEditor.isEditable = true;
  mockEditor.setEditable.mockClear();
});

function renderChapter(workId = 'w-123', chapter = 1) {
  return renderInApp(
    <Routes>
      <Route path="works/:workId/chapters/:chapter" element={<ChapterPage />} />
    </Routes>,
    {
      client: client(),
      initialRouterEntries: [`/works/${workId}/chapters/${chapter}`],
    },
  );
}

function chapterDetail(status: string, canEditOutline: boolean | 'absent' = true) {
  return http.get('/v1/local/works/:workId/chapters/:n', ({ params }) => {
    const detail: Record<string, unknown> = {
      work_id: params.workId,
      chapter: Number(params.n),
      volume: 1,
      slug: 'ch01',
      planned_word_count: 4000,
      status,
      can_edit_structure: true,
      body_read_only: true,
      created_at: '2026-06-25T00:00:00Z',
      updated_at: '2026-06-25T00:00:00Z',
    };
    if (canEditOutline !== 'absent') {
      detail.can_edit_outline = canEditOutline;
      detail.protection = canEditOutline
        ? { level: 'none', reason: '' }
        : { level: 'status', reason: 'Chapter is locked by the orchestration engine.' };
    }
    return HttpResponse.json(detail);
  });
}

describe('ChapterPage', () => {
  it('loads and renders the outline editor', async () => {
    useHandlers(
      chapterDetail('not_started'),
      http.get('/v1/local/works/:workId/chapters/:n/outline', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1\n\nOpening beat.',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          body_path: 'Works/WRK/Stories/ch01-ch01.md',
          content: 'Body prose.',
          frontmatter: { status: 'draft' },
          read_only: true,
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    renderChapter();
    expect(await screen.findByRole('heading', { name: 'Outline' })).toBeInTheDocument();
    expect(screen.getByLabelText('Outline editor')).toHaveValue('# Chapter 1\n\nOpening beat.');
  });

  it('shows the soft-concurrency warning for a draft chapter', async () => {
    useHandlers(
      chapterDetail('draft'),
      http.get('/v1/local/works/:workId/chapters/:n/outline', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          body_path: 'Works/WRK/Stories/ch01-ch01.md',
          content: 'Body prose.',
          read_only: true,
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    renderChapter();
    expect(await screen.findByText(/This chapter already has a draft body/i)).toBeInTheDocument();
  });

  it('saves the outline and updates the save-state indicator', async () => {
    let saved = false;
    useHandlers(
      chapterDetail('not_started'),
      http.get('/v1/local/works/:workId/chapters/:n/outline', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
      http.put('/v1/local/works/:workId/chapters/:n/outline', async ({ request }) => {
        const body = (await request.json()) as { content?: string };
        saved = true;
        return HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: body.content ?? '',
          updated_at: '2026-06-25T00:01:00Z',
        });
      }),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          body_path: 'Works/WRK/Stories/ch01-ch01.md',
          content: 'Body prose.',
          read_only: true,
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    renderChapter();
    await screen.findByText('Saved');
    const editor = screen.getByRole('textbox');
    await userEvent.type(editor, ' edited');
    await waitFor(() => expect(screen.getByText(/Unsaved changes/i)).toBeInTheDocument());
    await userEvent.click(screen.getByRole('button', { name: /Save Outline/i }));
    await waitFor(() => expect(saved).toBe(true));
    expect(screen.getByText('Saved')).toBeInTheDocument();
  });

  it('renders the body read-only and strips frontmatter', async () => {
    useHandlers(
      chapterDetail('draft'),
      http.get('/v1/local/works/:workId/chapters/:n/outline', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          body_path: 'Works/WRK/Stories/ch01-ch01.md',
          content: '---\nstatus: draft\n---\n\nBody prose.',
          frontmatter: { status: 'draft' },
          read_only: true,
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    renderChapter();
    await screen.findByRole('heading', { name: 'Outline' });
    await userEvent.click(screen.getByRole('tab', { name: /Body/i }));
    expect(await screen.findByText('Body prose.')).toBeInTheDocument();
    expect(screen.queryByText('---')).not.toBeInTheDocument();
    expect(screen.getByText(/Works\/WRK\/Stories\/ch01-ch01\.md/)).toBeInTheDocument();
  });

  it('copies the body path via the Copy Path button', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    useHandlers(
      chapterDetail('draft'),
      http.get('/v1/local/works/:workId/chapters/:n/outline', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          body_path: 'Works/WRK/Stories/ch01-ch01.md',
          content: 'Body prose.',
          read_only: true,
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    renderChapter();
    await screen.findByRole('heading', { name: 'Outline' });
    await userEvent.click(screen.getByRole('tab', { name: /Body/i }));
    await screen.findByText('Body prose.');
    await userEvent.click(screen.getByText('Copy Path'));
    expect(writeText).toHaveBeenCalledWith('Works/WRK/Stories/ch01-ch01.md');
  });

  it('shows the body error state and retry action', async () => {
    useHandlers(
      chapterDetail('draft'),
      http.get('/v1/local/works/:workId/chapters/:n/outline', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'boom' } },
          { status: 500 },
        ),
      ),
    );

    renderChapter();
    await screen.findByRole('heading', { name: 'Outline' });
    await userEvent.click(screen.getByRole('tab', { name: /Body/i }));
    expect(await screen.findByText('Could not load this view')).toBeInTheDocument();
    expect(screen.getByText(/Could not load the chapter body/i)).toBeInTheDocument();
  });

  it('opens the context menu on right-click and copies the path', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    useHandlers(
      chapterDetail('draft'),
      http.get('/v1/local/works/:workId/chapters/:n/outline', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          body_path: 'Works/WRK/Stories/ch01-ch01.md',
          content: 'Body prose.',
          read_only: true,
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    renderChapter();
    await screen.findByRole('heading', { name: 'Outline' });
    await userEvent.click(screen.getByRole('tab', { name: /Body/i }));
    await screen.findByText('Body prose.');
    const bodyRegion = screen.getByRole('region', { name: 'Chapter body' });
    await userEvent.pointer([{ keys: '[MouseRight]', target: bodyRegion }]);
    expect(await screen.findByRole('menu')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('menuitem', { name: /Copy Path/i }));
    expect(writeText).toHaveBeenCalledWith('Works/WRK/Stories/ch01-ch01.md');
  });

  it('closes the context menu with Escape and does not leak keydown listeners', async () => {
    const addListener = vi.spyOn(window, 'addEventListener');
    const removeListener = vi.spyOn(window, 'removeEventListener');

    useHandlers(
      chapterDetail('draft'),
      http.get('/v1/local/works/:workId/chapters/:n/outline', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          body_path: 'Works/WRK/Stories/ch01-ch01.md',
          content: 'Body prose.',
          read_only: true,
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    renderChapter();
    await screen.findByRole('heading', { name: 'Outline' });
    await userEvent.click(screen.getByRole('tab', { name: /Body/i }));
    await screen.findByText('Body prose.');
    const bodyRegion = screen.getByRole('region', { name: 'Chapter body' });

    addListener.mockClear();
    removeListener.mockClear();

    await userEvent.pointer([{ keys: '[MouseRight]', target: bodyRegion }]);
    expect(await screen.findByRole('menu')).toBeInTheDocument();

    // Escape closes the menu.
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument());

    // Re-open and close again to confirm the listener is re-attached cleanly.
    await userEvent.pointer([{ keys: '[MouseRight]', target: bodyRegion }]);
    expect(await screen.findByRole('menu')).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument());

    const keydownAdds = addListener.mock.calls.filter(([type]) => type === 'keydown').length;
    const keydownRemoves = removeListener.mock.calls.filter(([type]) => type === 'keydown').length;
    expect(keydownAdds).toBe(2);
    expect(keydownRemoves).toBe(2);

    addListener.mockRestore();
    removeListener.mockRestore();
  });

  it('shows the save error indicator when the outline PUT fails', async () => {
    useHandlers(
      chapterDetail('not_started'),
      http.get('/v1/local/works/:workId/chapters/:n/outline', () =>
        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
      http.put('/v1/local/works/:workId/chapters/:n/outline', () =>
        HttpResponse.json(
          { success: false, error: { code: 'conflict', message: 'Outline is locked' } },
          { status: 409 },
        ),
      ),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>
        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          body_path: 'Works/WRK/Stories/ch01-ch01.md',
          content: 'Body prose.',
          read_only: true,
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    renderChapter();
    await screen.findByText('Saved');
    const editor = screen.getByRole('textbox');
    await userEvent.type(editor, ' edited');
    await waitFor(() => expect(screen.getByText(/Unsaved changes/i)).toBeInTheDocument());
    await userEvent.click(screen.getByRole('button', { name: /Save Outline/i }));
    await waitFor(() =>
      expect(screen.getByLabelText('Save state')).toHaveTextContent(/Outline is locked/i),
    );
  });

  it('disables outline editing when can_edit_outline is false', async () => {
    useHandlers(
      chapterDetail('draft', false),
      http.get('/v1/local/works/:workId/chapters/:n/outline', () =>
        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>
        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          body_path: 'Works/WRK/Stories/ch01-ch01.md',
          content: 'Body prose.',
          read_only: true,
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    renderChapter();
    expect(
      await screen.findByText(/Outline editing is disabled for this chapter/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/Chapter is locked by the orchestration engine/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Save Outline/i })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Reset/i })).toBeDisabled();
    // R-V171-GREPTILE-P1-4: TipTap reads `editable` only at mount, so a cold
    // page load with the flag arriving late must still flip the editor to
    // read-only. Verify setEditable(false) was called.
    await waitFor(() => {
      expect(mockEditor.setEditable).toHaveBeenCalledWith(false);
    });
    expect(mockEditor.isEditable).toBe(false);
  });

  it('defaults to non-editable when can_edit_outline is absent (R-V171P1-QC1-003)', async () => {
    useHandlers(
      chapterDetail('draft', 'absent'),
      http.get('/v1/local/works/:workId/chapters/:n/outline', () =>
        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>
        HttpResponse.json({
          work_id: 'w-123',
          chapter: 1,
          volume: 1,
          body_path: 'Works/WRK/Stories/ch01-ch01.md',
          content: 'Body prose.',
          read_only: true,
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    renderChapter();
    expect(
      await screen.findByText(/Outline editing is disabled for this chapter/i),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Save Outline/i })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Reset/i })).toBeDisabled();
    await waitFor(() => {
      expect(mockEditor.setEditable).toHaveBeenCalledWith(false);
    });
    expect(mockEditor.isEditable).toBe(false);
  });
});
