/**
 * Outline alternate view — non-spatial companion to the spatial graph
 * (FB-C1-004, V1.108 P0 T3; V1.109 C2 T3 — Scene/Beat nested rows FB-C2-003;
 * V1.115 T1 — client-side sort controls R-V1108P0QC1-M001).
 *
 * Mirrors `strategy-alt-view.tsx`: every canvas must have a list/tree
 * companion so the outline is understandable without spatial navigation.
 * This renders:
 *   1. Chapters grouped by volume (then unassigned) with status badges.
 *      V1.109 C2 — Scene rows nest under each chapter; Beat rows nest under
 *      each scene (Scene→Beat nesting per §5.2 Q2). Type badges **Scene** /
 *      **Beat** distinguish the row kind. Chapters with zero scenes show the
 *      locked empty-under-chapter helper.
 *      V1.115 T1 — sortable by Number (flat numeric list) or Volume (grouped,
 *      the default). Sort is client-side and ephemeral (useState, never
 *      persisted); it reorders rows without mutating the underlying graph.
 *   2. Timeline events with descriptions and realized-chapter links.
 *      V1.115 T1 — sortable by Event time (timeline sequence). The WorkOutline
 *      projection carries no explicit timestamp on events, so "event time" is
 *      the canonical timeline sequence = array order (asc = declared order).
 *
 * Accessibility and productivity: keyboard users and screen readers get a
 * linear reading order. This is a read-only browse surface — editing stays
 * in the inspectors below, which remain visible in both graph and alt modes.
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Badge } from '@/components/ui/badge';
import {
  STATUS_LABEL_KEYS,
  SCENE_STATUS_LABEL_KEYS,
  STATUS_VARIANT,
  chapterDisplayTitle,
  unassignedChaptersOf,
} from './graph-projection';
import type { SceneBeatFixturePayload } from './graph-projection';
import type { ChapterSummary, WorkOutline } from '@42ch/nexus-contracts';

export interface OutlineAltViewProps {
  outline: WorkOutline;
  chapters: ChapterSummary[];
  /**
   * Optional Scene/Beat fixture (V1.109 C2 — FB-C2-003). When provided, Scene
   * rows nest under their parent chapter and Beat rows nest under their parent
   * scene. Empty/undefined on real Works — chapters then render with zero
   * scene/beat children (honest empty chrome).
   */
  sceneBeatFixture?: SceneBeatFixturePayload;
}

// V1.115 T1 — client-side sort state. Ephemeral: lives in useState and resets
// on every mount (never persisted). Lists are small and un-paginated.
type ChapterSortKey = 'volume' | 'number';
type SortDir = 'asc' | 'desc';

export function OutlineAltView({ outline, chapters, sceneBeatFixture }: OutlineAltViewProps) {
  const { t } = useTranslation('canvas');
  const titles = outline.chapter_titles as Record<string, string> | undefined;
  const chapterById = new Map<number, ChapterSummary>();
  for (const c of chapters) chapterById.set(c.chapter, c);
  const unassigned = unassignedChaptersOf(outline, chapters);

  // V1.109 C2 T3 — index scenes by chapter and beats by scene so the nested
  // rows render in O(chapters + scenes + beats) without re-scanning the
  // fixture per row. Empty when no fixture is provided (real Works).
  const hasSceneBeatFixture = sceneBeatFixture !== undefined;
  const scenesByChapter = new Map<number, SceneBeatFixturePayload['scenes']>();
  const beatsByScene = new Map<string, SceneBeatFixturePayload['beats']>();
  if (sceneBeatFixture) {
    for (const scene of sceneBeatFixture.scenes) {
      const bucket = scenesByChapter.get(scene.chapterId);
      if (bucket) bucket.push(scene);
      else scenesByChapter.set(scene.chapterId, [scene]);
    }
    for (const beat of sceneBeatFixture.beats) {
      const bucket = beatsByScene.get(beat.sceneId);
      if (bucket) bucket.push(beat);
      else beatsByScene.set(beat.sceneId, [beat]);
    }
  }

  // V1.115 T1 — chapter sort state (ephemeral). Default volume-asc reproduces
  // the historical grouped rendering exactly.
  const [chapterSortKey, setChapterSortKey] = useState<ChapterSortKey>('volume');
  const [chapterSortDir, setChapterSortDir] = useState<SortDir>('asc');
  // V1.115 T1 — timeline sort state. Only one sortable column (event time);
  // ascending is the declared timeline order.
  const [eventSortDir, setEventSortDir] = useState<SortDir>('asc');

  // Volume label per chapter — used to show volume membership inline when the
  // chapter list is flattened by the Number sort.
  const volumeLabelByChapter = new Map<number, string>();
  for (const v of outline.volumes) {
    const label = v.label || t('chapter.volume', { volume: v.volume_id });
    for (const id of v.chapter_ids) volumeLabelByChapter.set(id, label);
  }

  const isFlatNumber = chapterSortKey === 'number';

  // Volume-grouped path: reorder only the named-volume groups + unassigned
  // bucket. Chapter order WITHIN a volume stays the declared order (sorting by
  // volume is about group order, not intra-group order). Copies are taken so
  // the outline prop is never mutated.
  const volumesInOrder =
    chapterSortKey === 'volume' && chapterSortDir === 'desc'
      ? [...outline.volumes].reverse()
      : outline.volumes;
  const unassignedInOrder = chapterSortDir === 'desc' ? [...unassigned].reverse() : unassigned;

  // Number path: flatten every chapter (assigned + unassigned) into one list
  // sorted numerically. Volume membership is rendered inline per row.
  const flatChapters = isFlatNumber
    ? chapters
        .slice()
        .sort((a, b) => (chapterSortDir === 'asc' ? a.chapter - b.chapter : b.chapter - a.chapter))
    : [];

  // Timeline events — array order is the timeline sequence. Desc reverses it.
  const eventsInOrder =
    eventSortDir === 'desc' ? [...outline.timeline_events].reverse() : outline.timeline_events;

  function toggleChapterSort(key: ChapterSortKey) {
    if (key === chapterSortKey) {
      setChapterSortDir((d) => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      setChapterSortKey(key);
      setChapterSortDir('asc');
    }
  }
  function toggleEventSort() {
    setEventSortDir((d) => (d === 'asc' ? 'desc' : 'asc'));
  }

  // i18n sort fragments
  const dirAscWord = t('outlineAltView.sort.dirAsc');
  const dirDescWord = t('outlineAltView.sort.dirDesc');
  const colNumber = t('outlineAltView.sort.columnNumber');
  const colVolume = t('outlineAltView.sort.columnVolume');
  const colEventTime = t('outlineAltView.sort.columnEventTime');
  const chapterSortColumn = chapterSortKey === 'number' ? colNumber : colVolume;
  const chapterSortDirWord = chapterSortDir === 'asc' ? dirAscWord : dirDescWord;
  const eventSortDirWord = eventSortDir === 'asc' ? dirAscWord : dirDescWord;

  return (
    <section
      aria-label={t('outlineAltView.ariaLabel')}
      className="grid gap-4 lg:grid-cols-2"
    >
      {/* Chapter list grouped by volume */}
      <div className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card">
        <div className="flex items-center justify-between gap-2">
          <h3 className="text-heading-16 font-heading text-gray-1000">{t('outlineAltView.chaptersTitle')}</h3>
          <div
            role="group"
            aria-label={t('outlineAltView.sort.chapterGroupAria')}
            className="inline-flex items-center gap-1"
          >
            <SortButton
              label={colNumber}
              active={chapterSortKey === 'number'}
              dir={chapterSortDir}
              ariaLabel={sortButtonAria(
                chapterSortKey === 'number',
                colNumber,
                chapterSortDir,
                dirAscWord,
                dirDescWord,
                t,
              )}
              onClick={() => toggleChapterSort('number')}
            />
            <SortButton
              label={colVolume}
              active={chapterSortKey === 'volume'}
              dir={chapterSortDir}
              ariaLabel={sortButtonAria(
                chapterSortKey === 'volume',
                colVolume,
                chapterSortDir,
                dirAscWord,
                dirDescWord,
                t,
              )}
              onClick={() => toggleChapterSort('volume')}
            />
          </div>
        </div>
        <p className="sr-only">
          {t('outlineAltView.sort.chapterCaption', {
            sortKey: chapterSortColumn,
            sortDir: chapterSortDirWord,
          })}
        </p>

        {isFlatNumber ? (
          <ol className="mt-2 flex flex-col gap-1">
            {flatChapters.length === 0 ? (
              <li className="text-copy-13 text-gray-700">{t('outlineAltView.noChapters')}</li>
            ) : (
              flatChapters.map((chapter, i) => {
                const title = chapterDisplayTitle(chapter, titles, t('chapter.fallback'));
                const volumeLabel =
                  volumeLabelByChapter.get(chapter.chapter) ?? t('structureInspector.unassigned');
                return (
                  <ChapterRow
                    key={chapter.chapter}
                    chapter={chapter}
                    index={i + 1}
                    title={title}
                    volumeLabel={volumeLabel}
                    hasSceneBeatFixture={hasSceneBeatFixture}
                    scenesByChapter={scenesByChapter}
                    beatsByScene={beatsByScene}
                    t={t}
                  />
                );
              })
            )}
          </ol>
        ) : (
          <ol className="mt-2 flex flex-col gap-1">
            {volumesInOrder.map((volume) => (
              <li key={`vol-${volume.volume_id}`} className="flex flex-col gap-1">
                <span className="text-label-14 font-semibold text-gray-900">
                  {volume.label || t('chapter.volume', { volume: volume.volume_id })}
                </span>
                <ol className="ml-2 flex flex-col gap-1 border-l border-gray-alpha-300 pl-2">
                  {volume.chapter_ids.map((chapterId, i) => {
                    const chapter = chapterById.get(chapterId);
                    if (!chapter) return null;
                    const title = chapterDisplayTitle(chapter, titles, t('chapter.fallback'));
                    return (
                      <ChapterRow
                        key={chapterId}
                        chapter={chapter}
                        index={i + 1}
                        title={title}
                        hasSceneBeatFixture={hasSceneBeatFixture}
                        scenesByChapter={scenesByChapter}
                        beatsByScene={beatsByScene}
                        t={t}
                      />
                    );
                  })}
                </ol>
              </li>
            ))}
            {unassignedInOrder.length > 0 ? (
              <li className="flex flex-col gap-1">
                <span className="text-label-14 font-semibold text-gray-900">{t('structureInspector.unassigned')}</span>
                <ol className="ml-2 flex flex-col gap-1 border-l border-gray-alpha-300 pl-2">
                  {unassignedInOrder.map((chapter, i) => {
                    const title = chapterDisplayTitle(chapter, titles, t('chapter.fallback'));
                    return (
                      <ChapterRow
                        key={chapter.chapter}
                        chapter={chapter}
                        index={i + 1}
                        title={title}
                        hasSceneBeatFixture={hasSceneBeatFixture}
                        scenesByChapter={scenesByChapter}
                        beatsByScene={beatsByScene}
                        t={t}
                      />
                    );
                  })}
                </ol>
              </li>
            ) : null}
            {outline.volumes.length === 0 && unassigned.length === 0 ? (
              <li className="text-copy-13 text-gray-700">{t('outlineAltView.noChapters')}</li>
            ) : null}
          </ol>
        )}
      </div>

      {/* Timeline events list */}
      <div className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card">
        <div className="flex items-center justify-between gap-2">
          <h3 className="text-heading-16 font-heading text-gray-1000">{t('outlineAltView.timelineTitle')}</h3>
          <div
            role="group"
            aria-label={t('outlineAltView.sort.timelineGroupAria')}
            className="inline-flex items-center gap-1"
          >
            <SortButton
              label={colEventTime}
              active
              dir={eventSortDir}
              ariaLabel={sortButtonAria(true, colEventTime, eventSortDir, dirAscWord, dirDescWord, t)}
              onClick={toggleEventSort}
            />
          </div>
        </div>
        <p className="sr-only">
          {t('outlineAltView.sort.timelineCaption', { sortDir: eventSortDirWord })}
        </p>
        {outline.timeline_events.length === 0 ? (
          <p className="mt-2 text-copy-13 text-gray-700">{t('outlineAltView.noTimelineEvents')}</p>
        ) : (
          <ol className="mt-2 flex flex-col gap-1">
            {eventsInOrder.map((event, i) => (
              <li
                key={event.event_id}
                className="flex flex-col gap-0.5 rounded-control px-2 py-1 text-copy-14"
              >
                <span className="flex items-center gap-2">
                  <span className="w-6 shrink-0 text-copy-13-mono text-gray-700 tabular-nums">
                    {i + 1}.
                  </span>
                  <span className="font-medium text-gray-1000">{event.title}</span>
                </span>
                {event.description ? (
                  <span className="ml-8 text-copy-13 text-gray-700">{event.description}</span>
                ) : null}
                {event.realizes_chapter_id ? (
                  <span className="ml-8 text-label-12 text-gray-700">
                    {t('outlineAltView.realizesChapter', { chapter: event.realizes_chapter_id })}
                  </span>
                ) : null}
              </li>
            ))}
          </ol>
        )}
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// V1.115 T1 — Sort header button (R-V1108P0QC1-M001)
// ---------------------------------------------------------------------------

interface SortButtonProps {
  label: string;
  active: boolean;
  dir: SortDir;
  ariaLabel: string;
  onClick: () => void;
}

function SortButton({ label, active, dir, ariaLabel, onClick }: SortButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={ariaLabel}
      className={[
        'inline-flex items-center gap-1 rounded-control px-2 py-1 text-label-12 transition-colors duration-state ease-standard',
        active ? 'font-semibold text-gray-1000' : 'text-gray-700 hover:text-gray-1000',
      ].join(' ')}
    >
      <span>{label}</span>
      {active ? <span aria-hidden>{dir === 'asc' ? '▲' : '▼'}</span> : null}
    </button>
  );
}

function sortButtonAria(
  active: boolean,
  label: string,
  dir: SortDir,
  dirAscWord: string,
  dirDescWord: string,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  if (!active) return t('outlineAltView.sort.inactiveHint', { column: label });
  const dirWord = dir === 'asc' ? dirAscWord : dirDescWord;
  const nextWord = dir === 'asc' ? dirDescWord : dirAscWord;
  return t('outlineAltView.sort.activeHint', { column: label, dir: dirWord, next: nextWord });
}

// ---------------------------------------------------------------------------
// V1.109 C2 T3 — Chapter row + nested Scene/Beat rows (FB-C2-003)
// ---------------------------------------------------------------------------

interface ChapterRowProps {
  chapter: ChapterSummary;
  index: number;
  title: string;
  /**
   * V1.115 T1 — inline volume label shown when the chapter list is flattened
   * by the Number sort (so volume membership is not lost). Omitted in the
   * grouped view, where volume is already the group header.
   */
  volumeLabel?: string;
  /** True when a scene/beat fixture is active (gates the empty-under-chapter helper). */
  hasSceneBeatFixture: boolean;
  scenesByChapter: Map<number, SceneBeatFixturePayload['scenes']>;
  beatsByScene: Map<string, SceneBeatFixturePayload['beats']>;
  t: (key: string, options?: Record<string, unknown>) => string;
}

function ChapterRow({
  chapter,
  index,
  title,
  volumeLabel,
  hasSceneBeatFixture,
  scenesByChapter,
  beatsByScene,
  t,
}: ChapterRowProps) {
  const scenes = scenesByChapter.get(chapter.chapter) ?? [];
  return (
    <li className="flex flex-col gap-1">
      <div className="flex items-center gap-2 rounded-control px-2 py-1 text-copy-14">
        <span className="w-6 shrink-0 text-copy-13-mono text-gray-700 tabular-nums">
          {index}.
        </span>
        <span className="font-mono text-gray-700">#{chapter.chapter}</span>
        <span className="text-gray-1000">{title}</span>
        {volumeLabel ? (
          <span className="text-label-12 text-gray-700">{volumeLabel}</span>
        ) : null}
        <Badge variant={STATUS_VARIANT[chapter.status]}>
          {t(STATUS_LABEL_KEYS[chapter.status])}
        </Badge>
      </div>
      {scenes.length > 0 ? (
        <ol className="ml-8 flex flex-col gap-1 border-l border-gray-alpha-300 pl-2">
          {scenes.map((scene) => {
            const beats = beatsByScene.get(scene.sceneId) ?? [];
            const sceneTitle = scene.title?.trim() ? scene.title : t('outlineAltView.untitledScene');
            return (
              <li key={scene.sceneId} className="flex flex-col gap-1">
                <div className="flex items-center gap-2 rounded-control px-2 py-1 text-copy-14">
                  <Badge variant="neutral">{t('outlineAltView.sceneLabel')}</Badge>
                  <span className="text-gray-1000">{sceneTitle}</span>
                  {scene.status ? (
                    <Badge variant="neutral">{t(SCENE_STATUS_LABEL_KEYS[scene.status])}</Badge>
                  ) : null}
                </div>
                {beats.length > 0 ? (
                  <ol className="ml-8 flex flex-col gap-1 border-l border-gray-alpha-300 pl-2">
                    {beats.map((beat) => {
                      const beatTitle = beat.title?.trim() ? beat.title : t('outlineAltView.untitledBeat');
                      return (
                        <li
                          key={beat.beatId}
                          className="flex items-center gap-2 rounded-control px-2 py-1 text-copy-14"
                        >
                          <Badge variant="neutral">{t('outlineAltView.beatLabel')}</Badge>
                          <span className="text-gray-1000">{beatTitle}</span>
                          {beat.status ? (
                            <Badge variant="neutral">{t(SCENE_STATUS_LABEL_KEYS[beat.status])}</Badge>
                          ) : null}
                        </li>
                      );
                    })}
                  </ol>
                ) : null}
              </li>
            );
          })}
        </ol>
      ) : hasSceneBeatFixture ? (
        // Only show the empty-under-chapter helper when the fixture is active.
        // On real Works (no fixture), chapters render cleanly without the
        // helper — honest empty chrome (no invented scene structure).
        <p className="ml-8 text-copy-13 text-gray-700">{t('outlineAltView.noScenes')}</p>
      ) : null}
    </li>
  );
}
