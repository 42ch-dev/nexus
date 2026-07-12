/**
 * Outline alternate view — non-spatial companion to the spatial graph
 * (FB-C1-004, V1.108 P0 T3; V1.109 C2 T3 — Scene/Beat nested rows FB-C2-003).
 *
 * Mirrors `strategy-alt-view.tsx`: every canvas must have a list/tree
 * companion so the outline is understandable without spatial navigation.
 * This renders:
 *   1. Chapters grouped by volume (then unassigned) with status badges.
 *      V1.109 C2 — Scene rows nest under each chapter; Beat rows nest under
 *      each scene (Scene→Beat nesting per §5.2 Q2). Type badges **Scene** /
 *      **Beat** distinguish the row kind. Chapters with zero scenes show the
 *      locked empty-under-chapter helper.
 *   2. Timeline events with descriptions and realized-chapter links.
 *
 * Accessibility and productivity: keyboard users and screen readers get a
 * linear reading order. This is a read-only browse surface — editing stays
 * in the inspectors below, which remain visible in both graph and alt modes.
 */
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

  return (
    <section
      aria-label={t('outlineAltView.ariaLabel')}
      className="grid gap-4 lg:grid-cols-2"
    >
      {/* Chapter list grouped by volume */}
      <div className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card">
        <h3 className="text-heading-16 font-heading text-gray-1000">{t('outlineAltView.chaptersTitle')}</h3>
        <ol className="mt-2 flex flex-col gap-1">
          {outline.volumes.map((volume) => (
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
          {unassigned.length > 0 ? (
            <li className="flex flex-col gap-1">
              <span className="text-label-14 font-semibold text-gray-900">{t('structureInspector.unassigned')}</span>
              <ol className="ml-2 flex flex-col gap-1 border-l border-gray-alpha-300 pl-2">
                {unassigned.map((chapter, i) => {
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
      </div>

      {/* Timeline events list */}
      <div className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card">
        <h3 className="text-heading-16 font-heading text-gray-1000">{t('outlineAltView.timelineTitle')}</h3>
        {outline.timeline_events.length === 0 ? (
          <p className="mt-2 text-copy-13 text-gray-700">{t('outlineAltView.noTimelineEvents')}</p>
        ) : (
          <ol className="mt-2 flex flex-col gap-1">
            {outline.timeline_events.map((event, i) => (
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
// V1.109 C2 T3 — Chapter row + nested Scene/Beat rows (FB-C2-003)
// ---------------------------------------------------------------------------

interface ChapterRowProps {
  chapter: ChapterSummary;
  index: number;
  title: string;
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
        <Badge variant={STATUS_VARIANT[chapter.status]}>
          {t(STATUS_LABEL_KEYS[chapter.status])}
        </Badge>
      </div>
      {scenes.length > 0 ? (
        <ol className="ml-8 flex flex-col gap-1 border-l border-gray-alpha-300 pl-2">
          {scenes.map((scene) => {
            const beats = beatsByScene.get(scene.sceneId) ?? [];
            const sceneTitle = scene.title || t('outlineAltView.untitledScene');
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
                      const beatTitle = beat.title || t('outlineAltView.untitledBeat');
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
