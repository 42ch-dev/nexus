/**
 *
 * The outline structure inspector renders large chapter lists through
 * `react-window` FixedSizeList so only the visible viewport is mounted.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { i18n } from '@/lib/i18n/config';
import { OutlineStructurePanel } from './structure-inspector';
import type { ChapterSummary, WorkOutline } from '@42ch/nexus-contracts';

function chapter(id: number, overrides: Partial<ChapterSummary> = {}): ChapterSummary {
  return {
    chapter: id,
    title: `Chapter ${id}`,
    slug: `chapter-${id}`,
    status: 'not_started',
    planned_word_count: 1000,
    actual_word_count: undefined,
    volume: 1,
    work_id: 'wk_test',
    created_at: '',
    updated_at: '',
    ...overrides,
  };
}

function outline(overrides: Partial<WorkOutline> = {}): WorkOutline {
  return {
    work_id: 'wk_test',
    outline_revision: 1,
    volumes: [{ volume_id: 1, label: 'Volume 1', chapter_ids: [1, 2, 3] }],
    timeline_events: [],
    foreshadows: [],
    scenes: [],
    beats: [],
    chapter_titles: {},
    updated_at: '',
    ...overrides,
  };
}

describe('OutlineStructurePanel virtualization (R-V165-QC3-VIRT B6)', () => {
  it('renders a small volume chapter list and selects a chapter', async () => {
    const onSelect = vi.fn();
    const onMove = vi.fn();
    render(
      <OutlineStructurePanel
        outline={outline()}
        chapters={[chapter(1), chapter(2), chapter(3)]}
        selectedChapterId={null}
        onSelectChapter={onSelect}
        onMoveChapter={onMove}
      />,
    );

    expect(screen.getByText('Volume 1')).toBeInTheDocument();
    expect(screen.getByText('Chapter 2')).toBeInTheDocument();

    await userEvent.click(screen.getByText('Chapter 2'));
    expect(onSelect).toHaveBeenCalledWith(2);
  });

  it('renders the unassigned bucket separately', async () => {
    const onSelect = vi.fn();
    render(
      <OutlineStructurePanel
        outline={outline({ volumes: [] })}
        chapters={[chapter(10), chapter(11)]}
        selectedChapterId={null}
        onSelectChapter={onSelect}
        onMoveChapter={vi.fn()}
      />,
    );

    expect(screen.getByText('Unassigned')).toBeInTheDocument();
    await userEvent.click(screen.getByText('Chapter 11'));
    expect(onSelect).toHaveBeenCalledWith(11);
  });

  it('virtualizes a large volume list so not all rows are mounted', () => {
    const ids = Array.from({ length: 120 }, (_, i) => i + 1);
    render(
      <OutlineStructurePanel
        outline={outline({
          volumes: [{ volume_id: 1, label: 'Volume 1', chapter_ids: ids }],
        })}
        chapters={ids.map((id) => chapter(id))}
        selectedChapterId={null}
        onSelectChapter={vi.fn()}
        onMoveChapter={vi.fn()}
      />,
    );

    const buttons = screen.queryAllByRole('button', { name: /Chapter \d+/ });
    // should be mounted rather than all 120.
    expect(buttons.length).toBeGreaterThan(0);
    expect(buttons.length).toBeLessThan(120);
  });
});

describe('OutlineStructurePanel volume label', () => {
  afterEach(async () => {
    await i18n.changeLanguage('en');
  });

  it('renders the localized default for an unnamed volume under zh-CN', async () => {
    await i18n.changeLanguage('zh-CN');
    render(
      <OutlineStructurePanel
        outline={outline({ volumes: [{ volume_id: 1, label: '', chapter_ids: [1] }] })}
        chapters={[chapter(1)]}
        selectedChapterId={null}
        onSelectChapter={vi.fn()}
        onMoveChapter={vi.fn()}
      />,
    );

    expect(screen.getByRole('heading', { name: '第 1 卷' })).toBeInTheDocument();
  });
});
