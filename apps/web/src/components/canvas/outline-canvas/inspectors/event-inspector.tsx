/**
 * Outline canvas — event/timeline inspector (V1.73 B5 split,
 * `R-V172P0-QC1-002`; V1.108 P0 T4 foreshadow authoring — FB-C1-005).
 *
 * Renders the Work timeline: existing events with attach-to-chapter and
 * remove affordances, plus the "Add Event" composer and the foreshadow
 * link/unlink authoring controls. Drives the `patch_timeline_event` route.
 */
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowRight, CalendarPlus, Link2, Trash2, Unlink } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';

import type { TimelinePatchEventRequest, WorkOutline } from '@42ch/nexus-contracts';

interface TimelinePanelProps {
  outline: WorkOutline;
  selectedChapterId: number | null;
  baseRevision: number;
  onPatchTimeline: (request: TimelinePatchEventRequest) => void;
}

export function TimelinePanel({
  outline,
  selectedChapterId,
  baseRevision,
  onPatchTimeline,
}: TimelinePanelProps) {
  const { t } = useTranslation('canvas');
  const [newTitle, setNewTitle] = useState('');
  const [newDescription, setNewDescription] = useState('');
  // Per-source-event selected foreshadow target id (FB-C1-005 link control).
  const [linkTargetByEvent, setLinkTargetByEvent] = useState<Record<string, string>>({});

  // Foreshadow edges grouped by source event for quick lookup per row.
  const outgoingForeshadows = useMemo(() => {
    const map = new Map<string, string[]>();
    for (const link of outline.foreshadows) {
      const list = map.get(link.source_event_id) ?? [];
      list.push(link.target_event_id);
      map.set(link.source_event_id, list);
    }
    return map;
  }, [outline.foreshadows]);

  const eventTitleById = useMemo(() => {
    const map = new Map<string, string>();
    for (const event of outline.timeline_events) {
      map.set(event.event_id, event.title);
    }
    return map;
  }, [outline.timeline_events]);

  function addEvent() {
    if (!newTitle.trim()) return;
    onPatchTimeline({
      work_id: outline.work_id,
      base_revision: baseRevision,
      operation: 'add_event',
      title: newTitle.trim(),
      description: newDescription.trim() || undefined,
      realizes_chapter_id: selectedChapterId ?? undefined,
    });
    setNewTitle('');
    setNewDescription('');
  }

  function linkForeshadow(sourceEventId: string, targetEventId: string) {
    if (!targetEventId) return;
    onPatchTimeline({
      work_id: outline.work_id,
      base_revision: baseRevision,
      operation: 'link_foreshadow',
      event_id: sourceEventId,
      foreshadows_event_id: targetEventId,
    });
    setLinkTargetByEvent((prev) => {
      const next = { ...prev };
      delete next[sourceEventId];
      return next;
    });
  }

  function unlinkForeshadow(sourceEventId: string, targetEventId: string) {
    onPatchTimeline({
      work_id: outline.work_id,
      base_revision: baseRevision,
      operation: 'unlink_foreshadow',
      event_id: sourceEventId,
      foreshadows_event_id: targetEventId,
    });
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <CalendarPlus className="h-5 w-5 text-canvas-outline-timeline-marker" aria-hidden />
          {t('eventInspector.title')}
        </CardTitle>
        <CardDescription>{t('eventInspector.description')}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {outline.timeline_events.length === 0 ? (
          <p className="text-copy-13 text-gray-700">{t('outlineAltView.noTimelineEvents')}</p>
        ) : (
          <ul className="space-y-2">
            {outline.timeline_events.map((event) => {
              const targets = outgoingForeshadows.get(event.event_id) ?? [];
              const linkableEvents = outline.timeline_events.filter(
                (e) => e.event_id !== event.event_id && !targets.includes(e.event_id),
              );
              return (
                <li
                  key={event.event_id}
                  className="rounded-control border border-gray-alpha-300 bg-background-100 p-2"
                >
                  <div className="flex items-start justify-between">
                    <div>
                      <p className="text-copy-14 font-medium text-gray-1000">{event.title}</p>
                      {event.description ? (
                        <p className="text-copy-13 text-gray-700">{event.description}</p>
                      ) : null}
                      {event.realizes_chapter_id ? (
                        <p className="text-label-12 text-gray-700">
                          {t('outlineAltView.realizesChapter', { chapter: event.realizes_chapter_id })}
                        </p>
                      ) : null}
                    </div>
                    <div className="flex items-center gap-1">
                      {selectedChapterId && selectedChapterId !== event.realizes_chapter_id ? (
                        <button
                          type="button"
                          onClick={() =>
                            onPatchTimeline({
                              work_id: outline.work_id,
                              base_revision: baseRevision,
                              operation: 'attach_event_to_chapter',
                              event_id: event.event_id,
                              target_chapter_id: selectedChapterId,
                            })
                          }
                          className="rounded-control p-1 text-gray-700 hover:bg-gray-alpha-100"
                          aria-label={t('eventInspector.attachAria', { chapter: selectedChapterId })}
                          title={t('eventInspector.attachTitle')}
                        >
                          <Link2 className="h-4 w-4" aria-hidden />
                        </button>
                      ) : null}
                      <button
                        type="button"
                        onClick={() =>
                          onPatchTimeline({
                            work_id: outline.work_id,
                            base_revision: baseRevision,
                            operation: 'remove_event',
                            event_id: event.event_id,
                          })
                        }
                        className="rounded-control p-1 text-gray-700 hover:bg-gray-alpha-100"
                        aria-label={t('eventInspector.removeAria', { title: event.title })}
                        title={t('eventInspector.removeTitle')}
                      >
                        <Trash2 className="h-4 w-4" aria-hidden />
                      </button>
                    </div>
                  </div>

                  {targets.length > 0 ? (
                    <ul className="mt-1.5 space-y-1" aria-label={t('eventInspector.foreshadowsAria', { title: event.title })}>
                      {targets.map((targetId) => (
                        <li
                          key={targetId}
                          className="flex items-center justify-between gap-1 rounded-control bg-gray-alpha-100 px-1.5 py-0.5"
                        >
                          <span className="truncate text-label-12 text-gray-700">
                            {t('eventInspector.foreshadows', { title: eventTitleById.get(targetId) ?? targetId })}
                          </span>
                          <button
                            type="button"
                            onClick={() => unlinkForeshadow(event.event_id, targetId)}
                            className="flex shrink-0 items-center gap-1 rounded-control p-1 text-gray-700 hover:bg-gray-alpha-200"
                            aria-label={t('eventInspector.unlinkAria', { title: eventTitleById.get(targetId) ?? targetId })}
                            title={t('eventInspector.unlinkTitle')}
                          >
                            <Unlink className="h-3.5 w-3.5" aria-hidden />
                          </button>
                        </li>
                      ))}
                    </ul>
                  ) : null}

                  {linkableEvents.length > 0 ? (
                    <div className="mt-1.5 flex items-center gap-1.5">
                      <select
                        value={linkTargetByEvent[event.event_id] ?? ''}
                        onChange={(e) =>
                          setLinkTargetByEvent((prev) => ({
                            ...prev,
                            [event.event_id]: e.target.value,
                          }))
                        }
                        className="min-w-0 flex-1 rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-label-12 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700"
                        aria-label={t('eventInspector.targetAria', { title: event.title })}
                      >
                        <option value="">{t('eventInspector.targetPlaceholder')}</option>
                        {linkableEvents.map((target) => (
                          <option key={target.event_id} value={target.event_id}>
                            {target.title}
                          </option>
                        ))}
                      </select>
                      <Button
                        variant="secondary"
                        size="small"
                        onClick={() =>
                          linkForeshadow(
                            event.event_id,
                            linkTargetByEvent[event.event_id] ?? '',
                          )
                        }
                        disabled={!linkTargetByEvent[event.event_id]}
                      >
                        {t('eventInspector.link')}
                      </Button>
                    </div>
                  ) : null}
                </li>
              );
            })}
          </ul>
        )}

        <div className="rounded-card border border-gray-alpha-300 bg-background-100 p-3 space-y-2">
          <p className="text-label-14 font-semibold text-gray-900">{t('eventInspector.addTitle')}</p>
          <input
            type="text"
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            placeholder={t('eventInspector.titlePlaceholder')}
            className="w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700"
          />
          <textarea
            value={newDescription}
            onChange={(e) => setNewDescription(e.target.value)}
            placeholder={t('eventInspector.descriptionPlaceholder')}
            rows={2}
            className="w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700"
          />
          <Button variant="secondary" size="small" onClick={addEvent} disabled={!newTitle.trim()}>
            <ArrowRight className="h-4 w-4" aria-hidden /> {t('eventInspector.addButton')}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
