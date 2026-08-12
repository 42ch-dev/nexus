/**
 * Timeline inspector — inline title/body editor for a selected Timeline node
 * (V1.122 P1 T4) + cross-surface navigation affordance (V1.123 P3 Task 4).
 *
 * Edits a World-scoped KeyBlock entity via the orchestrator's `onPatchEntity`
 * callback, which routes the patch through `NexusClient.worldKbPatchEntity`
 * (V1.73 `POST .../kb/patch-entity`) only. The adapter owns no write state;
 * the orchestrator's React Query mutation is the single write path.
 *
 * Architect-locked write boundary (§4.2): the inspector MUST NOT invoke
 * `timeline.patch_event` (Work-scoped), `world_kb.patch_relationship`
 * (read-only on Timeline), `kb.promote_candidate` (World KB surface), or any
 * raw-file write. The negative assertions in
 * `timeline-write-boundary.test.tsx` enforce this.
 *
 * Validation UX (422): when the orchestrator's mutation returns
 * `world_kb_validation_failed`, the inspector renders the
 * `validation_summary.errors[]` inline (mirrors the V1.73 entity inspector).
 * Conflict UX (409): handed off to the orchestrator via `onConflict`, which
 * opens the world-kb-flavored `WorldKbEntityConflictModal`.
 *
 * V1.123 P3 Task 4 — event nodes (`layoutHint === 'event'`) ALSO surface a
 * "View in Work Timeline" affordance when a realizing Work is bound
 * (`ctxRef.current.boundWorkId` + `ctxRef.current.onViewInWorkTimeline` both
 * present). The affordance hides when either slot is absent (honest scope
 * cut per plan §"If binding is missing or unreliable, P3 hides the
 * affordance"). Context (non-event) nodes do NOT surface the CTA even when a
 * realizing Work exists — the cross-surface binding axis is event-only.
 */
import { useEffect, useState, type MutableRefObject } from 'react';
import { useTranslation } from 'react-i18next';
import { BookOpen } from 'lucide-react';
import type { Node } from '@xyflow/react';

import { Textarea } from '@/components/ui/textarea';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { BLOCK_TYPE_LABELS } from '../world-kb/types';
import type { BlockType } from '@42ch/nexus-contracts';
import type {
  TimelineCanvasAdapterContext,
  TimelineEntityPatch,
  TimelinePatchField,
} from './timeline-canvas-adapter';
import type { TimelineNodeData } from './timeline-canvas-adapter';

/** Editable form derived from a selected Timeline node's backing projection. */
interface TimelineEditForm {
  title: string;
  bodyText: string; // JSON-serialised body (free-form Record<string, unknown>)
}

function formFromNode(data: TimelineNodeData): TimelineEditForm {
  return {
    title: data.canonical_name ?? '',
    bodyText: data.body ? JSON.stringify(data.body, null, 2) : '',
  };
}

/** Which form fields differ from the node's canonical projection. */
function computeDirty(
  form: TimelineEditForm,
  data: TimelineNodeData,
): TimelinePatchField[] {
  const dirty: TimelinePatchField[] = [];
  if (form.title !== (data.canonical_name ?? '')) dirty.push('title');
  const canonBody = data.body ? JSON.stringify(data.body, null, 2) : '';
  if (form.bodyText !== canonBody) dirty.push('body');
  return dirty;
}

export interface TimelineInspectorProps {
  node: Node<TimelineNodeData>;
  ctxRef: MutableRefObject<TimelineCanvasAdapterContext>;
}

export function TimelineInspector({ node, ctxRef }: TimelineInspectorProps) {
  const { t } = useTranslation('canvas');
  const ctx = ctxRef.current;
  const data = node.data;
  const [form, setForm] = useState<TimelineEditForm>(() => formFromNode(data));
  const [validationErrors, setValidationErrors] = useState<string[]>([]);
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Reseed the form when the selected node changes.
  useEffect(() => {
    setForm(formFromNode(data));
    setValidationErrors([]);
    setIsSubmitting(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data.key_block_id, data.version]);

  const dirty = computeDirty(form, data);
  const blockTypeLabel =
    BLOCK_TYPE_LABELS[data.block_type as BlockType] ?? data.block_type;

  async function handleSubmit() {
    if (dirty.length === 0) return;
    setValidationErrors([]);

    const patch: TimelineEntityPatch = {};
    if (dirty.includes('title')) patch.title = form.title.trim();
    if (dirty.includes('body')) {
      try {
        if (!form.bodyText.trim()) {
          // Empty body is a no-op for the wire DTO; skip rather than emit
          // `undefined` so the patch is well-formed.
        } else {
          const parsed: unknown = JSON.parse(form.bodyText);
          if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
            throw new Error('body must be a JSON object');
          }
          patch.body = parsed as Record<string, unknown>;
        }
      } catch (err) {
        setValidationErrors([
          err instanceof Error && err.message === 'body must be a JSON object'
            ? t('timeline.inspector.bodyJsonObjectError')
            : t('timeline.inspector.bodyJsonError'),
        ]);
        return;
      }
    }

    const onPatch = ctx.onPatchEntity;
    if (typeof onPatch !== 'function') {
      // No write hook wired (e.g. read-only test mounts). Render the form
      // but do not attempt the write — surfaces the inspector chrome without
      // a phantom mutation.
      return;
    }

    setIsSubmitting(true);
    // The orchestrator's mutation owns the actual `worldKbPatchEntity` call,
    // invalidation, conflict hand-off, and refetch. The inspector only
    // forwards the structured patch + dirty fields, and awaits the returned
    // promise so `isSubmitting` clears on EVERY outcome — success AND error
    // (PR #156: previously the flag stayed `true` on 409/422/network failure
    // because only a successful version-bump reseed would reset it, leaving
    // Save permanently disabled until the selection changed).
    try {
      await onPatch(node, patch, dirty);
    } catch {
      // The orchestrator's mutation `onError` already surfaces conflict /
      // validation / toast UX. The inspector only owns the local submit
      // flag, which `finally` resets so the author can retry immediately.
    } finally {
      setIsSubmitting(false);
    }
  }

  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        handleSubmit();
      }}
      aria-label={t('timeline.inspector.aria', { name: data.canonical_name })}
    >
      <div className="flex items-center justify-between gap-2">
        <h3
          className="text-heading-16 font-heading text-gray-1000"
          // eslint-disable-next-line react/forbid-dom-props
          data-testid="timeline-inspector-title"
        >
          {t('timeline.inspector.title')}
        </h3>
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {t('timeline.inspector.version', { version: data.version })}
        </span>
      </div>
      <p className="text-copy-13 text-gray-700">
        {t('timeline.inspector.description')}
      </p>

      <div className="flex flex-col gap-1">
        <Label htmlFor="tl-title">
          {t('timeline.inspector.field.title')}
        </Label>
        <Input
          id="tl-title"
          value={form.title}
          onChange={(e) => setForm((prev) => ({ ...prev, title: e.target.value }))}
        />
      </div>

      <div className="flex flex-col gap-1">
        <Label htmlFor="tl-blocktype">
          {t('timeline.inspector.field.blockType')}
        </Label>
        <Input
          id="tl-blocktype"
          value={blockTypeLabel}
          readOnly
          aria-readonly
          // The Timeline surface does not change entity block_type (that's
          // a World KB promotion operation). The field renders read-only so
          // the author sees the entity kind on this surface without being
          // invited to mutate it from here.
        />
      </div>

      <div className="flex flex-col gap-1">
        <Label htmlFor="tl-body">{t('timeline.inspector.field.body')}</Label>
        <Textarea
          id="tl-body"
          rows={6}
          className="font-mono text-copy-13-mono"
          value={form.bodyText}
          onChange={(e) =>
            setForm((prev) => ({ ...prev, bodyText: e.target.value }))
          }
          placeholder={t('timeline.inspector.bodyPlaceholder')}
          spellCheck={false}
        />
      </div>

      {validationErrors.length > 0 ? (
        <ul
          className="rounded-card border border-red-700/30 bg-red-700/10 p-3 text-copy-13 text-red-1000"
          aria-live="polite"
          data-testid="timeline-inspector-validation-errors"
        >
          {validationErrors.map((err, i) => (
            <li key={i}>{err}</li>
          ))}
        </ul>
      ) : null}

      {/* V1.123 P3 Task 4 — cross-surface navigation affordance.
          Reserved for event nodes (`layoutHint === 'event'`) — the
          cross-surface binding axis is World-event ↔ Work-event only;
          context entities (characters, locations) do not surface this CTA
          even when a realizing Work is bound. The affordance also hides
          when the orchestrator has not supplied `boundWorkId` + the
          navigation callback (honest scope cut per plan §). */}
      {data.layoutHint === 'event' && ctx.boundWorkId && ctx.onViewInWorkTimeline ? (
        <button
          type="button"
          data-testid="timeline-view-in-work-timeline"
          data-work-id={ctx.boundWorkId}
          // V1.163 P1 Task 2 — when the selected World event has an
          // event-level bind (`boundWorkEventId`), the CTA target deep-links
          // to the specific Work outline event
          // (`?layer=narrative&event=<id>`); absent → the V1.123 surface-level
          // jump (`?layer=narrative` only). The attribute carries the event
          // id so the DOM exposes which target the click will land on.
          data-event-id={ctx.boundWorkEventId}
          onClick={ctx.onViewInWorkTimeline}
          aria-label={t('timeline.inspector.viewInWorkTimelineAria', {
            defaultValue: 'Open the Work that realizes this World on the Work Timeline',
          })}
          className="inline-flex items-center gap-1.5 self-start rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 shadow-elevation-2 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
        >
          <BookOpen className="h-3.5 w-3.5" aria-hidden />
          {t('timeline.inspector.viewInWorkTimeline', {
            defaultValue: 'View in Work Timeline',
          })}
        </button>
      ) : null}

      <div className="flex items-center justify-between gap-2">
        <span className="text-label-12 text-gray-700">
          {dirty.length === 0
            ? t('timeline.inspector.noChanges')
            : t('timeline.inspector.editing', {
                fields: dirty
                  .map((d) => t(`timeline.inspector.field.${d}`))
                  .join(', '),
              })}
        </span>
        <Button
          type="submit"
          disabled={dirty.length === 0 || isSubmitting}
          data-testid="timeline-inspector-save"
        >
          {isSubmitting
            ? t('timeline.inspector.saving')
            : t('timeline.inspector.save')}
        </Button>
      </div>
    </form>
  );
}
