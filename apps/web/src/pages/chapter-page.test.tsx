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

function chapterDetail(status: string) {
  return http.get('/v1/local/works/:workId/chapters/:n', ({ params }) =>    HttpResponse.json({
      work_id: params.workId,
      chapter: Number(params.n),
      volume: 1,
      slug: 'ch01',
      planned_word_count: 4000,
      status,
      can_edit_outline: true,
      can_edit_structure: true,
      body_read_only: true,
      protection: { level: 'none', reason: '' },
      created_at: '2026-06-25T00:00:00Z',
      updated_at: '2026-06-25T00:00:00Z',
    }),
  );
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
});
