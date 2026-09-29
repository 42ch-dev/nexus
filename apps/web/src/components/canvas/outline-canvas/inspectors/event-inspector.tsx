/**
 * Outline canvas — event/timeline inspector (V1.73 B5 split,
 * `R-V172P0-QC1-002`; V1.108 P0 T4 foreshadow authoring — FB-C1-005;
 * V1.200 DR-26 cross-surface World-event binding authoring).
 *
 * Renders the Work timeline: existing events with attach-to-chapter and
 * remove affordances, plus the "Add Event" composer, the foreshadow
 * link/unlink authoring controls, and the World-event bind/unbind control.
 * Drives the `patch_timeline_event` route.
 */
import { useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowRight, CalendarPlus, Link2, Trash2, Unlink } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { useWorldKbGraph } from '@/lib/canvas/use-world-kb-data';

import type {
  TimelinePatchEventRequest,
  WorkOutline,
  WorldKbGraphResponse,
} from '@42ch/nexus-contracts';

interface TimelinePanelProps {
  outline: WorkOutline;
  selectedChapterId: number | null;
  baseRevision: number;
  /**
   * Dispatch a `timeline.patch_event` write. `onSuccess` fires only after the
   * write lands, so the bind drafts below are cleared on success alone
   * (V1.200 DR-26 round 2 — a 409/422 refusal keeps the typed draft).
   */
  onPatchTimeline: (request: TimelinePatchEventRequest, onSuccess?: () => void) => void;
  /**
   * The Work's bound World id (`Work.world_id`, V1.200 DR-26). Absent → the
   * World-event binding control renders **disabled, never hidden**: the core
   * refuses both `bind_world_event` and `unbind_world_event` without a bound
   * World, so the affordance stays visible and carries the refusal reason.
   */
  boundWorldId?: string;
}

/** One selectable World KB `block_type=event` entity of the bound World. */
interface WorldEventOption {
  id: string;
  name: string;
}

/**
 * The two controls that author a World-event binding draft (V1.200 DR-26): the
 * picker's selection and the manual World-event-ID field.
 */
type WorldEventDraftControl = 'picker' | 'manual';

/**
 * Project the bound World's KB graph into picker options (V1.200 DR-26).
 *
 * Reuses the existing single graph source (`useWorldKbGraph` →
 * `WorldKbGraphResponse.entities[]`, V1.73) — the same read the World Timeline
 * Narrative layer projects — so no new World KB endpoint or query hook is
 * introduced. Non-`event` entities are filtered out because the core refuses
 * every other referent `block_type`.
 */
function worldEventOptionsFromGraph(graph: WorldKbGraphResponse | undefined): WorldEventOption[] {
  if (!graph) return [];
  return graph.entities
    .filter((entity) => entity.block_type === 'event')
    .map((entity) => ({ id: entity.key_block_id, name: entity.canonical_name }));
}

/**
 * Timeline panel entry point.
 *
 * V1.201 002/R3 — the entry returns one component type regardless of
 * `boundWorldId`. Splitting it on the id's truthiness (`BoundWorldTimelinePanel`
 * when bound, `TimelinePanelView` when not) made React swap the child's
 * component type when the Work's World resolved after first paint, remounting
 * the view and discarding in-progress drafts. The graph read is hoisted here
 * instead, so the view's identity survives the transition.
 *
 * The bound-World KB graph is still read only when a World is bound: the query
 * is gated on the id inside `useWorldKbGraph` (`enabled: Boolean(worldId)`), so
 * the unbound panel issues no World read — the graph query is the picker's
 * option source, not a second source of truth.
 */
export function TimelinePanel(props: TimelinePanelProps) {
  const { boundWorldId } = props;
  const graph = useWorldKbGraph(boundWorldId);
  const worldEventOptions = useMemo(() => worldEventOptionsFromGraph(graph.data), [graph.data]);
  return <TimelinePanelView {...props} worldEventOptions={worldEventOptions} />;
}

function TimelinePanelView({
  outline,
  selectedChapterId,
  baseRevision,
  onPatchTimeline,
  boundWorldId,
  worldEventOptions,
}: TimelinePanelProps & { worldEventOptions: WorldEventOption[] }) {
  const { t } = useTranslation('canvas');
  const [newTitle, setNewTitle] = useState('');
  const [newDescription, setNewDescription] = useState('');
  // Per-source-event selected foreshadow target id (FB-C1-005 link control).
  const [linkTargetByEvent, setLinkTargetByEvent] = useState<Record<string, string>>({});
  // Per-event selected World-event target id (V1.200 DR-26 bind control).
  const [worldEventTargetByEvent, setWorldEventTargetByEvent] = useState<Record<string, string>>({});
  const [manualWorldEventByEvent, setManualWorldEventByEvent] = useState<Record<string, string>>({});

  // V1.201 002/R4 — one draft generation per (control, event). The value alone
  // is not a draft tag: a manual `A → B → A` retype reads as untouched, and a
  // picker draft can equal an independently typed manual one. Every draft
  // mutation bumps its generation, so a resolving bind can tell whether the
  // draft it was issued against is still the draft in the field.
  const draftGenerationRef = useRef<Record<WorldEventDraftControl, Record<string, number>>>({
    picker: {},
    manual: {},
  });

  const draftSetters = {
    picker: setWorldEventTargetByEvent,
    manual: setManualWorldEventByEvent,
  } as const;

  function setWorldEventDraft(
    control: WorldEventDraftControl,
    eventId: string,
    value: string,
  ) {
    const byEvent = draftGenerationRef.current[control];
    byEvent[eventId] = (byEvent[eventId] ?? 0) + 1;
    draftSetters[control]((prev) => ({ ...prev, [eventId]: value }));
  }

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

  // Bound referent → display name, so a binding renders its World event's
  // canonical name when the (possibly capped) graph read carries it and falls
  // back to the raw `key_block_id` otherwise.
  const worldEventNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const option of worldEventOptions) {
      map.set(option.id, option.name);
    }
    return map;
  }, [worldEventOptions]);

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

  function bindWorldEvent(
    control: WorldEventDraftControl,
    eventId: string,
    worldEventId: string,
  ) {
    const trimmedId = worldEventId.trim();
    if (!trimmedId) return;
    // V1.201 002/R4 — a bind is issued against exactly one control's draft, so
    // the request tag is that control's draft generation. The write is still
    // cleared only once it lands (the success-only pattern the scene/beat
    // drafts use); a typed 422/409 refusal keeps the draft for retry, and the
    // 409 conflict modal still opens through the orchestrator's `onError`.
    const issuedGeneration = draftGenerationRef.current[control][eventId] ?? 0;
    onPatchTimeline(
      {
        work_id: outline.work_id,
        base_revision: baseRevision,
        operation: 'bind_world_event',
        event_id: eventId,
        world_event_id: trimmedId,
      },
      // V1.201 002/R4 — release exactly the draft this bind was issued
      // against: the originating control, and only while its generation is
      // unchanged (a re-typed draft is a newer generation and survives). The
      // other control's draft was never part of this request, so an unrelated
      // bind's success must leave it alone.
      () => {
        if ((draftGenerationRef.current[control][eventId] ?? 0) !== issuedGeneration) {
          return;
        }
        draftSetters[control]((prev) => {
          if (!(eventId in prev)) return prev;
          const next = { ...prev };
          delete next[eventId];
          return next;
        });
      },
    );
  }

  function unbindWorldEvent(eventId: string) {
    onPatchTimeline({
      work_id: outline.work_id,
      base_revision: baseRevision,
      operation: 'unbind_world_event',
      event_id: eventId,
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
        {boundWorldId ? (
          <CardDescription>{t('eventInspector.worldEventPickerLimit')}</CardDescription>
        ) : null}
      </CardHeader>
      <CardContent className="space-y-4">
        {!boundWorldId ? (
          <p className="text-label-12 text-gray-700">{t('eventInspector.worldEventRequired')}</p>
        ) : null}

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

                  {event.world_event_id ? (
                    <div className="mt-1.5 flex items-center gap-1.5">
                      <span className="min-w-0 flex-1 truncate text-label-12 text-gray-700">
                        {t('eventInspector.worldEventBound', {
                          name: worldEventNameById.get(event.world_event_id) ?? event.world_event_id,
                        })}
                      </span>
                      <Button
                        variant="secondary"
                        size="small"
                        onClick={() => unbindWorldEvent(event.event_id)}
                        disabled={!boundWorldId}
                        title={
                          boundWorldId
                            ? t('eventInspector.worldEventUnbindTitle')
                            : t('eventInspector.worldEventRequired')
                        }
                      >
                        {t('eventInspector.worldEventUnbind')}
                      </Button>
                    </div>
                  ) : (
                    <div className="mt-1.5 space-y-1.5">
                      <div className="flex items-center gap-1.5">
                        <select
                          value={worldEventTargetByEvent[event.event_id] ?? ''}
                          onChange={(e) =>
                            setWorldEventDraft('picker', event.event_id, e.target.value)
                          }
                          disabled={!boundWorldId}
                          title={boundWorldId ? undefined : t('eventInspector.worldEventRequired')}
                          className="min-w-0 flex-1 rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-label-12 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700"
                          aria-label={t('eventInspector.worldEventAria', { title: event.title })}
                        >
                          <option value="">{t('eventInspector.worldEventPlaceholder')}</option>
                          {worldEventOptions.map((option) => (
                            <option key={option.id} value={option.id}>
                              {option.name}
                            </option>
                          ))}
                        </select>
                        <Button
                          variant="secondary"
                          size="small"
                          onClick={() =>
                            bindWorldEvent(
                              'picker',
                              event.event_id,
                              worldEventTargetByEvent[event.event_id] ?? '',
                            )
                          }
                          disabled={!boundWorldId || !worldEventTargetByEvent[event.event_id]}
                        >
                          {t('eventInspector.worldEventBind')}
                        </Button>
                      </div>
                      <div className="flex items-center gap-1.5">
                        <input
                          type="text"
                          value={manualWorldEventByEvent[event.event_id] ?? ''}
                          onChange={(e) =>
                            setWorldEventDraft('manual', event.event_id, e.target.value)
                          }
                          disabled={!boundWorldId}
                          placeholder={t('eventInspector.worldEventManualPlaceholder')}
                          aria-label={t('eventInspector.worldEventManualAria', { title: event.title })}
                          className="min-w-0 flex-1 rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-label-12 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700"
                        />
                        <Button
                          variant="secondary"
                          size="small"
                          onClick={() =>
                            bindWorldEvent(
                              'manual',
                              event.event_id,
                              manualWorldEventByEvent[event.event_id] ?? '',
                            )
                          }
                          disabled={
                            !boundWorldId || !manualWorldEventByEvent[event.event_id]?.trim()
                          }
                        >
                          {t('eventInspector.worldEventManualBind')}
                        </Button>
                      </div>
                    </div>
                  )}
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
