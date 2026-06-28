/**
 * Unit tests for the outline-canvas pure projection helpers (V1.73 B5).
 *
 * These cover the logic extracted from the original monolith so the projection
 * rules (unassigned bucket, display-title fallback, conflict field projection)
 * have dedicated coverage independent of the React rendering.
 */
import { describe, expect, it } from 'vitest';

import {
  STATUS_OPTIONS,
  STATUS_VARIANT,
  changedFieldsOf,
  chapterDisplayTitle,
  unassignedChaptersOf,
} from '../graph-projection';
import type {
  ChapterSummary,
  WorkOutline,
} from '@42ch/nexus-contracts';

function chapter(partial: Partial<ChapterSummary>): ChapterSummary {
  return {
    chapter: 1,
    title: undefined,
    slug: undefined,
    status: 'not_started',
    planned_word_count: 0,
    actual_word_count: undefined,
    volume: 1,
    ...partial,
  } as ChapterSummary;
}

function outline(volumes: WorkOutline['volumes']): WorkOutline {
  return {
    work_id: 'wk_test',
    outline_revision: 0,
    volumes,
    timeline_events: [],
    foreshadows: [],
    chapter_titles: {},
    updated_at: '',
  };
}

describe('STATUS_OPTIONS / STATUS_VARIANT', () => {
  it('exposes all five lifecycle statuses', () => {
    const values = STATUS_OPTIONS.map((o) => o.value).sort();
    expect(values).toEqual(['draft', 'finalized', 'not_started', 'outlined', 'published']);
  });

  it('maps every status onto a Badge variant', () => {
    for (const option of STATUS_OPTIONS) {
      expect(STATUS_VARIANT[option.value]).toBeTruthy();
    }
  });
});

describe('unassignedChaptersOf', () => {
  it('returns chapters referenced by no volume', () => {
    const result = unassignedChaptersOf(
      outline([{ volume_id: 1, label: 'Volume 1', chapter_ids: [1] }]),
      [chapter({ chapter: 1 }), chapter({ chapter: 2 }), chapter({ chapter: 3 })],
    );
    expect(result.map((c) => c.chapter)).toEqual([2, 3]);
  });

  it('returns an empty list when every chapter is assigned', () => {
    const result = unassignedChaptersOf(
      outline([{ volume_id: 1, label: 'Volume 1', chapter_ids: [1, 2] }]),
      [chapter({ chapter: 1 }), chapter({ chapter: 2 })],
    );
    expect(result).toEqual([]);
  });
});

describe('chapterDisplayTitle', () => {
  it('prefers the outline chapter_titles map', () => {
    expect(
      chapterDisplayTitle(chapter({ chapter: 4, title: 'db title' }), { '4': 'UI Title' }),
    ).toBe('UI Title');
  });

  it('falls back to the chapter title then a localized fallback', () => {
    expect(chapterDisplayTitle(chapter({ chapter: 7, title: 'db title' }), undefined)).toBe(
      'db title',
    );
    expect(chapterDisplayTitle(chapter({ chapter: 7, title: undefined }), undefined)).toBe(
      'Chapter 7',
    );
  });
});

describe('changedFieldsOf', () => {
  it('projects structure operations by kind', () => {
    expect(
      changedFieldsOf({
        kind: 'structure',
        request: {
          work_id: 'wk',
          base_revision: 0,
          operation: 'move_chapter',
          chapter_id: 1,
          volume_id: 2,
        },
      }),
    ).toEqual(['move_chapter']);
  });

  it('projects timeline operations by kind', () => {
    expect(
      changedFieldsOf({
        kind: 'timeline',
        request: { work_id: 'wk', base_revision: 0, operation: 'remove_event', event_id: 'e1' },
      }),
    ).toEqual(['remove_event']);
  });

  it('projects each edited chapter set field, in a stable order', () => {
    expect(
      changedFieldsOf({
        kind: 'chapter',
        chapter: 1,
        request: {
          work_id: 'wk',
          chapter_id: 1,
          base_revision: 0,
          set: { title: 'T', slug: 't', volume: 2 },
        },
      }),
    ).toEqual(['chapter_title', 'chapter_slug', 'chapter_volume']);
  });

  it('omits untouched chapter fields', () => {
    expect(
      changedFieldsOf({
        kind: 'chapter',
        chapter: 1,
        request: { work_id: 'wk', chapter_id: 1, base_revision: 0, set: {} },
      }),
    ).toEqual([]);
  });
});
