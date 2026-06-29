/**
 * Outline canvas — event/timeline inspector (V1.73 B5 split,
 * `R-V172P0-QC1-002`).
 *
 * Renders the Work timeline: existing events with attach-to-chapter and
 * remove affordances, plus the "Add Event" composer. Drives the
 * `patch_timeline_event` route. Extracted from the original `outline-canvas.tsx`
 * monolith; behavior is unchanged.
 */
import { useState } from 'react';
import { ArrowRight, CalendarPlus, Link2, Trash2 } from 'lucide-react';

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
  const [newTitle, setNewTitle] = useState('');
  const [newDescription, setNewDescription] = useState('');

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

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <CalendarPlus className="h-5 w-5 text-teal-700" aria-hidden />
          Timeline
        </CardTitle>
        <CardDescription>Events, beats, and foreshadow links.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {outline.timeline_events.length === 0 ? (
          <p className="text-copy-13 text-gray-700">No timeline events yet.</p>
        ) : (
          <ul className="space-y-2">
            {outline.timeline_events.map((event) => (
              <li
                key={event.event_id}
                className="flex items-start justify-between rounded-control border border-gray-alpha-300 bg-background-100 p-2"
              >
                <div>
                  <p className="text-copy-14 font-medium text-gray-1000">{event.title}</p>
                  {event.description ? (
                    <p className="text-copy-13 text-gray-700">{event.description}</p>
                  ) : null}
                  {event.realizes_chapter_id ? (
                    <p className="text-label-12 text-gray-700">
                      Chapter {event.realizes_chapter_id}
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
                          realizes_chapter_id: selectedChapterId,
                        })
                      }
                      className="rounded-control p-1 text-gray-700 hover:bg-gray-alpha-100"
                      aria-label={`Attach event to chapter ${selectedChapterId}`}
                      title="Attach to selected chapter"
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
                    aria-label={`Remove event ${event.title}`}
                    title="Remove event"
                  >
                    <Trash2 className="h-4 w-4" aria-hidden />
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}

        <div className="rounded-card border border-gray-alpha-300 bg-background-100 p-3 space-y-2">
          <p className="text-label-14 font-semibold text-gray-900">Add Event</p>
          <input
            type="text"
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            placeholder="Event title…"
            className="w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-700"
          />
          <textarea
            value={newDescription}
            onChange={(e) => setNewDescription(e.target.value)}
            placeholder="Description (optional)…"
            rows={2}
            className="w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-700"
          />
          <Button variant="secondary" size="small" onClick={addEvent} disabled={!newTitle.trim()}>
            <ArrowRight className="h-4 w-4" aria-hidden /> Add to timeline
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
