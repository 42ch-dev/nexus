/**
 * Outline alternate view — non-spatial companion to the spatial graph
 * (FB-C1-004, V1.108 P0 T3).
 *
 * Mirrors `strategy-alt-view.tsx`: every canvas must have a list/tree
 * companion so the outline is understandable without spatial navigation.
 * This renders:
 *   1. Chapters grouped by volume (then unassigned) with status badges.
 *   2. Timeline events with descriptions and realized-chapter links.
 *
 * Accessibility and productivity: keyboard users and screen readers get a
 * linear reading order. This is a read-only browse surface — editing stays
 * in the inspectors below, which remain visible in both graph and alt modes.
 */
import { Badge } from '@/components/ui/badge';
import {
  STATUS_VARIANT,
  chapterDisplayTitle,
  unassignedChaptersOf,
} from './graph-projection';
import type { ChapterSummary, WorkOutline } from '@42ch/nexus-contracts';

export interface OutlineAltViewProps {
  outline: WorkOutline;
  chapters: ChapterSummary[];
}

export function OutlineAltView({ outline, chapters }: OutlineAltViewProps) {
  const titles = outline.chapter_titles as Record<string, string> | undefined;
  const chapterById = new Map<number, ChapterSummary>();
  for (const c of chapters) chapterById.set(c.chapter, c);
  const unassigned = unassignedChaptersOf(outline, chapters);

  return (
    <section
      aria-label="Outline chapters and timeline in list order"
      className="grid gap-4 lg:grid-cols-2"
    >
      {/* Chapter list grouped by volume */}
      <div className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card">
        <h3 className="text-heading-16 font-heading text-gray-1000">Chapters</h3>
        <ol className="mt-2 flex flex-col gap-1">
          {outline.volumes.map((volume) => (
            <li key={`vol-${volume.volume_id}`} className="flex flex-col gap-1">
              <span className="text-label-14 font-semibold text-gray-900">
                {volume.label || `Volume ${volume.volume_id}`}
              </span>
              <ol className="ml-2 flex flex-col gap-1 border-l border-gray-alpha-300 pl-2">
                {volume.chapter_ids.map((chapterId, i) => {
                  const chapter = chapterById.get(chapterId);
                  if (!chapter) return null;
                  const title = chapterDisplayTitle(chapter, titles);
                  return (
                    <li
                      key={chapterId}
                      className="flex items-center gap-2 rounded-control px-2 py-1 text-copy-14"
                    >
                      <span className="w-6 shrink-0 text-copy-13-mono text-gray-700 tabular-nums">
                        {i + 1}.
                      </span>
                      <span className="font-mono text-gray-700">#{chapter.chapter}</span>
                      <span className="text-gray-1000">{title}</span>
                      <Badge variant={STATUS_VARIANT[chapter.status]}>
                        {chapter.status.replace(/_/g, ' ')}
                      </Badge>
                    </li>
                  );
                })}
              </ol>
            </li>
          ))}
          {unassigned.length > 0 ? (
            <li className="flex flex-col gap-1">
              <span className="text-label-14 font-semibold text-gray-900">Unassigned</span>
              <ol className="ml-2 flex flex-col gap-1 border-l border-gray-alpha-300 pl-2">
                {unassigned.map((chapter) => {
                  const title = chapterDisplayTitle(chapter, titles);
                  return (
                    <li
                      key={chapter.chapter}
                      className="flex items-center gap-2 rounded-control px-2 py-1 text-copy-14"
                    >
                      <span className="font-mono text-gray-700">#{chapter.chapter}</span>
                      <span className="text-gray-1000">{title}</span>
                      <Badge variant={STATUS_VARIANT[chapter.status]}>
                        {chapter.status.replace(/_/g, ' ')}
                      </Badge>
                    </li>
                  );
                })}
              </ol>
            </li>
          ) : null}
          {outline.volumes.length === 0 && unassigned.length === 0 ? (
            <li className="text-copy-13 text-gray-700">No chapters yet.</li>
          ) : null}
        </ol>
      </div>

      {/* Timeline events list */}
      <div className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card">
        <h3 className="text-heading-16 font-heading text-gray-1000">Timeline Events</h3>
        {outline.timeline_events.length === 0 ? (
          <p className="mt-2 text-copy-13 text-gray-700">No timeline events yet.</p>
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
                    Realizes chapter {event.realizes_chapter_id}
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
