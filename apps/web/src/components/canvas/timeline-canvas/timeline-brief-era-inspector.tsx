/**
 * Timeline Brief-era inspector — V1.123 P1 T4 (Brief-layer feel
 * differentiation).
 *
 * Surfaces era markers (`eraId`, `startHint`, `endHint`, `worldSummary`)
 * extracted from `body.attributes` per architect §2.3 + §8, prominently in
 * a dedicated chrome distinct from the generic Narrative event inspector
 * (`TimelineInspector` — title + body JSON editor).
 *
 * Architect-locked write boundary (§4.2): the inspector routes patches
 * through the orchestrator's `onPatchEntity` callback, which calls
 * `NexusClient.worldKbPatchEntity` (V1.73 `POST .../kb/patch-entity`)
 * ONLY. The same write path as the Narrative inspector — what changes is
 * the chrome: era markers are surfaced as first-class read-only fields
 * alongside the title editor; the body JSON editor remains available for
 * advanced editing of the full body (including `attributes.start_hint`
 * etc.). Era marker fields themselves are NOT individually editable in P1
 * — direct editing of structured `attributes.*` is a P4 era-carrier task
 * (Brief-on-carrier); P1 surfaces them as honest read-only identity
 * fields.
 *
 * Visual differentiation from `TimelineInspector`:
 *   - Distinct `data-testid` so the dispatch contract is testable.
 *   - Era id pill rendered first (era identity), then the time-span
 *     label, then the world summary displayed in full (the card chrome
 *     truncates to 2 lines; the inspector shows the whole text).
 *   - The title editor + body JSON editor live below the era marker
 *     block so the era identity reads first.
 *
 * The component does NOT invoke any forbidden write method
 * (`timeline.patch_event`, `world_kb.patch_relationship`,
 * `kb.promote_candidate`, raw-file writes) — same boundary as
 * `TimelineInspector`.
 */
import { useEffect, useState, type MutableRefObject } from 'react';
import { useTranslation } from 'react-i18next';
import { Hourglass } from 'lucide-react';
import type { Node } from '@xyflow/react';

import { Textarea } from '@/components/ui/textarea';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import type {
  TimelineCanvasAdapterContext,
  TimelineEntityPatch,
  TimelinePatchField,
} from './timeline-canvas-adapter';
import type { TimelineNodeData } from './timeline-canvas-adapter';

interface BriefEraEditForm {
  title: string;
  bodyText: string;
}

function formFromNode(data: TimelineNodeData): BriefEraEditForm {
  return {
    title: data.canonical_name ?? '',
    bodyText: data.body ? JSON.stringify(data.body, null, 2) : '',
  };
}

function computeDirty(
  form: BriefEraEditForm,
  data: TimelineNodeData,
): TimelinePatchField[] {
  const dirty: TimelinePatchField[] = [];
  if (form.title !== (data.canonical_name ?? '')) dirty.push('title');
  const canonBody = data.body ? JSON.stringify(data.body, null, 2) : '';
  if (form.bodyText !== canonBody) dirty.push('body');
  return dirty;
}

export interface TimelineBriefEraInspectorProps {
  node: Node<TimelineNodeData>;
  ctxRef: MutableRefObject<TimelineCanvasAdapterContext>;
}

export function TimelineBriefEraInspector({
  node,
  ctxRef,
}: TimelineBriefEraInspectorProps) {
  const { t } = useTranslation('canvas');
  const ctx = ctxRef.current;
  const data = node.data;
  const [form, setForm] = useState<BriefEraEditForm>(() => formFromNode(data));
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

  // Era marker fields are surfaced as honest read-only identity. Direct
  // editing of structured `attributes.*` is a P4 era-carrier task; P1
  // surfaces them so the author sees the era's identity at a glance. The
  // values come from the adapter's `extractEraAttributes` projection —
  // they are already on `TimelineNodeData` as top-level fields.
  const eraId = data.eraId;
  const startHint = data.startHint;
  const endHint = data.endHint;
  const worldSummary = data.worldSummary;

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
      // No write hook wired (e.g. read-only test mounts). Render the chrome
      // but do not attempt the write — same guard as `TimelineInspector`.
      return;
    }

    setIsSubmitting(true);
    // Same write-path semantics as `TimelineInspector`: the orchestrator's
    // mutation owns the actual `worldKbPatchEntity` call, invalidation,
    // conflict hand-off, and refetch. The inspector forwards the patch +
    // dirty fields and awaits the returned promise so `isSubmitting`
    // clears on EVERY outcome (PR #156 fix).
    try {
      await onPatch(node, patch, dirty);
    } catch {
      // Orchestrator's mutation `onError` surfaces conflict / validation /
      // toast UX. Inspector only owns the local submit flag.
    } finally {
      setIsSubmitting(false);
    }
  }

  // The time-span label mirrors the Brief-era node card: prefer
  // `start_hint → end_hint`; fall back to whichever hint exists; fall back
  // to the temporal-unknown label when neither is present.
  const span = (() => {
    if (startHint && endHint) {
      return t('timeline.briefEraNode.span', { start: startHint, end: endHint });
    }
    if (startHint) return startHint;
    if (endHint) return endHint;
    return t('timeline.briefEraNode.temporalUnknown');
  })();

  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        handleSubmit();
      }}
      aria-label={t('timeline.briefEraInspector.aria', { name: data.canonical_name })}
      data-testid="timeline-brief-era-inspector"
    >
      <div className="flex items-center justify-between gap-2">
        <h3
          className="flex items-center gap-2 text-heading-16 font-heading text-gray-1000"
          // eslint-disable-next-line react/forbid-dom-props
          data-testid="timeline-brief-era-inspector-title"
        >
          <Hourglass
            className="h-4 w-4 flex-shrink-0 text-canvas-worldkb-accent"
            aria-hidden
          />
          {t('timeline.briefEraInspector.title')}
        </h3>
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {t('timeline.inspector.version', { version: data.version })}
        </span>
      </div>
      <p className="text-copy-13 text-gray-700">
        {t('timeline.briefEraInspector.description')}
      </p>

      {/* Era identity block — read-only marker fields surface the era's
          identity prominently above the title editor. Per the Brief feel
          contract (layer-feel §2.2), these are the era's distinguishing
          fields; they read first. */}
      <div
        className="flex flex-col gap-2 rounded-card border border-gray-alpha-400 bg-background-100 p-3"
        aria-label={t('timeline.briefEraInspector.identityAria')}
      >
        {eraId ? (
          <div className="flex flex-col gap-1">
            <span className="text-label-12 font-semibold uppercase tracking-wide text-gray-700">
              {t('timeline.briefEraInspector.field.eraId')}
            </span>
            <span
              className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-900 self-start"
              // eslint-disable-next-line react/forbid-dom-props
              data-testid="timeline-brief-era-inspector-era-id"
            >
              {eraId}
            </span>
          </div>
        ) : null}

        <div className="flex flex-col gap-1">
          <span className="text-label-12 font-semibold uppercase tracking-wide text-gray-700">
            {t('timeline.briefEraInspector.field.span')}
          </span>
          <span
            className="rounded-pill border border-canvas-worldkb-accent/30 bg-canvas-worldkb-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-worldkb-accent self-start"
            // eslint-disable-next-line react/forbid-dom-props
            data-testid="timeline-brief-era-inspector-span"
          >
            {span}
          </span>
        </div>

        {worldSummary ? (
          <div className="flex flex-col gap-1">
            <span className="text-label-12 font-semibold uppercase tracking-wide text-gray-700">
              {t('timeline.briefEraInspector.field.worldSummary')}
            </span>
            <p
              className="text-copy-13 text-gray-900"
              // eslint-disable-next-line react/forbid-dom-props
              data-testid="timeline-brief-era-inspector-world-summary"
            >
              {worldSummary}
            </p>
          </div>
        ) : null}
      </div>

      {/* Title editor — the era's canonical name. Routes through the same
          `kb.patch_entity` write path as the Narrative inspector. */}
      <div className="flex flex-col gap-1">
        <Label htmlFor="tl-brief-era-title">
          {t('timeline.inspector.field.title')}
        </Label>
        <Input
          id="tl-brief-era-title"
          value={form.title}
          onChange={(e) => setForm((prev) => ({ ...prev, title: e.target.value }))}
        />
      </div>

      {/* Body JSON editor — advanced editing of the full body (including
          `attributes.start_hint` etc.). The structured era marker fields
          above are read-only for P1; this editor is the escape hatch for
          authors who need direct body edits before the P4 era carrier. */}
      <div className="flex flex-col gap-1">
        <Label htmlFor="tl-brief-era-body">
          {t('timeline.inspector.field.body')}
        </Label>
        <Textarea
          id="tl-brief-era-body"
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
          data-testid="timeline-brief-era-inspector-validation-errors"
        >
          {validationErrors.map((err, i) => (
            <li key={i}>{err}</li>
          ))}
        </ul>
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
          data-testid="timeline-brief-era-inspector-save"
        >
          {isSubmitting
            ? t('timeline.inspector.saving')
            : t('timeline.inspector.save')}
        </Button>
      </div>
    </form>
  );
}
