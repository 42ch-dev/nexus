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
import { HOLDER_BLOCK_TYPES } from '../world-kb/entity-inspector';
import type { BlockType } from '@42ch/nexus-contracts';
import type {
  TimelineCanvasAdapterContext,
  TimelineEntityPatch,
  TimelinePatchField,
} from './timeline-canvas-adapter';
import { extractTimelineConflict, mapTimelineValidationEntry } from './timeline-canvas-adapter';
import type { TimelineNodeData } from './timeline-canvas-adapter';

/** Editable form derived from a selected Timeline node's backing projection. */
interface TimelineEditForm {
  title: string;
  bodyText: string; // JSON-serialised body (free-form Record<string, unknown>)
  /**
   * v1.203 P2 O3 — `modules.observation.observers` as entry-id strings in
   * author order. Graph-resolved names render beside the id; ids absent from
   * the loaded graph stay raw (PD-18 — no new fan-out fetch).
   */
  observers: string[];
  /**
   * PD-9 explicit empty claim (C3 fix): true only when the author
   * deliberately claimed "no observers" (Clear / claim checkbox) —
   * distinct from an untouched unrecorded seed, which keeps the key
   * omitted. Submitting with this flag writes `observers: []`, including
   * on the first save from a previously unrecorded event.
   */
  observersClaimNone: boolean;
  /** `modules.observation.access` as JSON text ('' = omit / clear). */
  accessText: string;
  /**
   * C1 fix — raw-JSON fallback mode: non-null when the stored observation
   * member has a shape the structured editor cannot represent verbatim
   * (non-object module, non-array / mixed-type observers, non-object
   * access). Holds the JSON text of the COMPLETE stored value; untouched
   * seeds stay byte-identical (not dirty) until the author edits.
   */
  observationRawText: string | null;
}

/**
 * Stored `modules.observation` member EXACTLY as it rides the projection
 * (`{ present: false }` when the key is absent). The dirty check compares
 * against this raw member — never a coerced copy — so untouched unusual
 * seeds can never go dirty through normalization (C1).
 */
function storedObservation(data: TimelineNodeData): {
  present: boolean;
  value: unknown;
} {
  const modules = data.modules;
  if (
    modules === null ||
    typeof modules !== 'object' ||
    Array.isArray(modules) ||
    !('observation' in modules)
  ) {
    return { present: false, value: undefined };
  }
  return {
    present: true,
    value: (modules as Record<string, unknown>).observation,
  };
}

/**
 * C1 — whether the structured editor can represent the stored observation
 * member VERBATIM: a plain object whose `observers` (when present) is a
 * string array and whose `access` (when present) is a plain object. Any
 * other stored shape (non-object module, non-array / mixed-type observers,
 * `access: null`, …) is nonrepresentable → raw-JSON fallback, never silent
 * coercion or dropping.
 */
function observationShapeSupported(observation: Record<string, unknown>): boolean {
  const observers = observation.observers;
  if (
    observers !== undefined &&
    (!Array.isArray(observers) || observers.some((o) => typeof o !== 'string'))
  ) {
    return false;
  }
  const access = observation.access;
  if (
    access !== undefined &&
    (access === null || typeof access !== 'object' || Array.isArray(access))
  ) {
    return false;
  }
  return true;
}

function formFromNode(data: TimelineNodeData): TimelineEditForm {
  const { present, value } = storedObservation(data);
  const supported =
    present &&
    value !== null &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    observationShapeSupported(value as Record<string, unknown>);
  if (!supported) {
    return {
      title: data.canonical_name ?? '',
      bodyText: data.body ? JSON.stringify(data.body, null, 2) : '',
      observers: [],
      observersClaimNone: false,
      accessText: '',
      // Raw-JSON fallback (C1): seed with the COMPLETE stored value so an
      // untouched unusual seed is not dirty; the author edits it as JSON.
      observationRawText: present ? JSON.stringify(value, null, 2) : null,
    };
  }
  const observation = value as Record<string, unknown>;
  const storedObservers = observation.observers;
  return {
    title: data.canonical_name ?? '',
    bodyText: data.body ? JSON.stringify(data.body, null, 2) : '',
    // Stored string ids seed VERBATIM — no trim/coercion — so a
    // whitespace-containing stored id (legal under the frozen string-array
    // contract) is not rewritten by an unrelated save (C1).
    observers: Array.isArray(storedObservers)
      ? storedObservers.filter((o): o is string => typeof o === 'string')
      : [],
    observersClaimNone: false,
    accessText:
      observation.access !== undefined && observation.access !== null
        ? JSON.stringify(observation.access, null, 2)
        : '',
    observationRawText: null,
  };
}

/**
 * Build the complete first-level `modules.observation` value for the
 * whole-value upsert (AR-4/PD-12). Two modes:
 *
 * Raw fallback (C1) — `form.observationRawText !== null` when the stored
 * member was nonrepresentable. The parsed JSON object becomes the value
 * verbatim (unknown inner keys round-trip untouched, PD-13); invalid JSON
 * or a non-object parse keeps the raw text in the built value so the exact
 * dirty check still sees the edit (mirrors T2's `order` handling) while
 * the error blocks the write.
 *
 * Structured — unknown inner keys round-trip verbatim (PD-13 — the value
 * starts from the stored record). Observer ids write VERBATIM from the
 * form (stored ids are seeded unchanged; author-entered ids are trimmed at
 * entry time), so untouched stored ids are never normalized. Observer
 * semantics follow PD-9: the author's explicit "no observers" claim
 * (`observersClaimNone`, set by Clear / the claim checkbox) writes the
 * explicit empty container, while a stored record that never carried
 * `observers` and an emptied-but-unclaimed selection keeps the key absent
 * (unrecorded ≠ explicitly nobody). Access: blank clears to the empty
 * object (PD-16 empty-object omission reads as absent); invalid JSON keeps
 * the raw text in the built value while the error blocks the write.
 *
 * Returns per-field errors keyed `observation.<field>` (the 422 mapping key
 * shape).
 */
function buildObservationValue(
  stored: Record<string, unknown>,
  form: TimelineEditForm,
  accessJsonError: (field: string) => string,
): { value: Record<string, unknown>; errors: Record<string, string> } {
  const errors: Record<string, string> = {};
  if (form.observationRawText !== null) {
    const rawText = form.observationRawText.trim();
    try {
      const parsed: unknown = JSON.parse(rawText);
      if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
        throw new Error('observation must be a JSON object');
      }
      return { value: parsed as Record<string, unknown>, errors };
    } catch {
      // Keep the raw text in the built value so the exact dirty check
      // still sees the edit (mirrors the access invalid-JSON handling).
      return {
        value: { raw: rawText },
        errors: { 'observation.raw': accessJsonError('observation') },
      };
    }
  }
  const value: Record<string, unknown> = { ...stored };
  if (form.observersClaimNone) {
    value.observers = [];
  } else if ('observers' in stored || form.observers.length > 0) {
    value.observers = form.observers;
  } else {
    delete value.observers;
  }
  const accessText = form.accessText.trim();
  if (!accessText) {
    if ('access' in stored) value.access = {};
    else delete value.access;
  } else {
    try {
      const parsed: unknown = JSON.parse(accessText);
      if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
        throw new Error('access must be a JSON object');
      }
      value.access = parsed as Record<string, unknown>;
    } catch {
      value.access = accessText;
      errors['observation.access'] = accessJsonError('access');
    }
  }
  return { value, errors };
}

/** Exact dirty check against the raw stored member (no coercion — C1). */
function observationDirty(form: TimelineEditForm, data: TimelineNodeData): boolean {
  const { present, value } = storedObservation(data);
  const storedRaw = present ? value : {};
  // Raw fallback mode: compare the edited TEXT to the seeded text. An
  // untouched seed (even a non-object module, whose parse can never yield
  // the stored value) is NOT dirty, so an unrelated title-only save is
  // never blocked and never rewrites modules. Editing marks dirty; the
  // build then requires a valid JSON object before the write proceeds.
  if (form.observationRawText !== null) {
    return (
      form.observationRawText.trim() !== JSON.stringify(storedRaw, null, 2).trim()
    );
  }
  const stored =
    typeof storedRaw === 'object' && storedRaw !== null && !Array.isArray(storedRaw)
      ? (storedRaw as Record<string, unknown>)
      : {};
  const built = buildObservationValue(stored, form, () => '');
  return JSON.stringify(built.value) !== JSON.stringify(storedRaw);
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
  // C2 — the observation module axis is event-only (mirrors the JSX gate):
  // context nodes never report or emit `modules`, no matter what unusual
  // stored observation shape rides the projection.
  if (data.layoutHint === 'event' && observationDirty(form, data)) dirty.push('modules');
  return dirty;
}

/**
 * Resolve observer entry_ids → canonical names using ONLY entities already
 * in the loaded graph (`ctx.nodes` — the projected `surface.nodes` the
 * orchestrator supplies, each carrying `key_block_id` + `canonical_name`).
 * PD-18: no new fan-out fetch solely for this panel — ids without a name in
 * memory render raw. Mirrors the Task 2 fixture's `name (id)` format.
 */
function observerLabels(
  observers: unknown[],
  ctx: TimelineCanvasAdapterContext,
): string[] {
  const names = new Map<string, string>();
  for (const node of ctx.nodes ?? []) {
    const id = node.data.key_block_id;
    const name = node.data.canonical_name;
    if (id && name) names.set(id, name);
  }
  return observers.map((observer) => {
    const id = String(observer);
    const name = names.get(id);
    return name !== undefined && name !== id ? `${name} (${id})` : id;
  });
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
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const [rawObserver, setRawObserver] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Reseed the form when the selected node changes.
  useEffect(() => {
    setForm(formFromNode(data));
    setValidationErrors([]);
    setFieldErrors({});
    setIsSubmitting(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data.key_block_id, data.version]);

  const dirty = computeDirty(form, data);
  const blockTypeLabel =
    BLOCK_TYPE_LABELS[data.block_type as BlockType] ?? data.block_type;

  // V1.164 P3 Task 4 — observation modules ride on the event's KB projection
  // (`data.modules`, additive Task 1 wire field; TimelineNodeData extends
  // WorldKbEntityProjection). Null / non-object guards keep the read safe on
  // every wire state; `observers` stays `undefined` (→ no line) unless a
  // present observation object actually carries it.
  const modules = data.modules;
  const observation =
    modules !== null && typeof modules === 'object'
      ? (modules as Record<string, unknown>).observation
      : undefined;
  const observers =
    observation !== null && typeof observation === 'object'
      ? (observation as Record<string, unknown>).observers
      : undefined;

  // v1.203 P2 O3 — observer picker options come from entities ALREADY in the
  // loaded graph (PD-18 — no new fan-out fetch), holder-capable kinds only
  // (the observation carrier is a holder/character axis, V1.164 PD-8/PD-12);
  // the observed event itself is excluded. Selected ids absent from the
  // graph render as extra raw checked rows so they stay removable (PD-18
  // raw-id read-back); new ids enter via the raw-id input below.
  const observerOptions = (ctx.nodes ?? [])
    .map((n) => n.data)
    .filter(
      (nodeData) =>
        typeof nodeData.key_block_id === 'string' &&
        nodeData.key_block_id.length > 0 &&
        nodeData.key_block_id !== data.key_block_id &&
        HOLDER_BLOCK_TYPES.includes(nodeData.block_type as BlockType),
    );
  const optionIds = new Set(observerOptions.map((nodeData) => nodeData.key_block_id));
  const rawSelectedObservers = form.observers.filter((id) => !optionIds.has(id));

  function toggleObserver(id: string) {
    setForm((prev) => {
      const selected = prev.observers.includes(id);
      return {
        ...prev,
        // Selecting a concrete observer supersedes a "no observers" claim.
        observersClaimNone: selected ? prev.observersClaimNone : false,
        observers: selected
          ? prev.observers.filter((observer) => observer !== id)
          : [...prev.observers, id],
      };
    });
  }

  function addRawObserver() {
    const id = rawObserver.trim();
    if (!id) return;
    setForm((prev) =>
      prev.observers.includes(id)
        ? prev
        : { ...prev, observersClaimNone: false, observers: [...prev.observers, id] },
    );
    setRawObserver('');
  }

  async function handleSubmit() {
    if (dirty.length === 0) return;
    setValidationErrors([]);
    setFieldErrors({});

    const patch: TimelineEntityPatch = {};
    if (dirty.includes('title')) patch.title = form.title.trim();
    // C2 — defense in depth: module patch construction is event-only even
    // if a caller ever passes a dirty list computed without the gate.
    if (data.layoutHint === 'event' && dirty.includes('modules')) {
      // v1.203 P2 O3 — whole-first-level-value upsert of
      // `modules.observation` (AR-4/PD-12). Only this dialect is authored
      // from the Timeline surface; the daemon's first-level merge preserves
      // sibling dialects. `audience` governance is never emitted here.
      const { present, value } = storedObservation(data);
      const storedRaw = present ? value : {};
      const stored =
        typeof storedRaw === 'object' && storedRaw !== null && !Array.isArray(storedRaw)
          ? (storedRaw as Record<string, unknown>)
          : {};
      const built = buildObservationValue(stored, form, (field) =>
        t('timeline.inspector.observation.accessJsonError', { field }),
      );
      if (Object.keys(built.errors).length > 0) {
        setFieldErrors(built.errors);
        return;
      }
      patch.modules = { observation: built.value };
    }
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
    } catch (err) {
      // The orchestrator's mutation `onError` already surfaces the section
      // banner (conflict modal / validation list / toast UX). The inspector
      // additionally maps daemon 422 entries carrying the frozen prefix
      // grammar onto the offending observation form fields (write contract
      // §3 — same discipline as T2's `mapValidationEntry`); entries that
      // address anything else stay section-level verbatim.
      const info = extractTimelineConflict(err);
      if (info?.kind === 'validation') {
        const mapped = info.errors.map(mapTimelineValidationEntry);
        setFieldErrors(
          Object.fromEntries(
            mapped.filter((m) => m.key).map((m) => [m.key as string, m.message]),
          ),
        );
        setValidationErrors(mapped.filter((m) => !m.key).map((m) => m.message));
      }
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

      {/* V1.164 P3 Task 4 — read-only observers metadata line (PD-9 / PD-18).
          `modules.observation.observers` rides on the event's KB projection
          (TimelineNodeData extends WorldKbEntityProjection; the daemon ships
          `modules` verbatim). Absent `modules` / `modules.observation` =
          unrecorded → the line is skipped entirely; `observers: []` =
          explicitly nobody → the explicit "No observers" claim renders;
          malformed (non-array) observers is skipped leniently (mirrors the
          Task 2 fixture + P2 checker). Names resolve only from the already
          loaded graph — no new fetch.
          S-4 (QC fix wave) — the observers axis is narrative-event-only:
          observation semantics describe who witnessed an EVENT, and this
          inspector is shared with context nodes (characters/locations), so
          the line is gated to `layoutHint === 'event'`. */}
      {data.layoutHint === 'event' && Array.isArray(observers) ? (
        <p className="mt-2 text-copy-13" data-testid="event-observers-line">
          <span className="font-semibold text-gray-900">
            {t('timeline.inspector.observers')}
          </span>{' '}
          {observers.length === 0 ? (
            <span className="text-gray-1000">
              {t('timeline.inspector.noObservers')}
            </span>
          ) : (
            <span className="font-mono text-gray-1000">
              {observerLabels(observers, ctx).join(', ')}
            </span>
          )}
        </p>
      ) : null}

      {/* v1.203 P2 O3 — Observation edit affordance (write contract O3).
          Event nodes only (`layoutHint === 'event'` — the existing read-path
          gate mirrors the O1/O2 holder-gate discipline); context nodes never
          surface it. Whole-first-level-value upsert of
          `modules.observation` on save — see buildObservationValue. */}
      {data.layoutHint === 'event' ? (
        <section
          className="mt-3 flex flex-col gap-2 border-t border-gray-alpha-300 pt-2"
          data-testid="observation-section"
          aria-label={t('timeline.inspector.observation.title')}
        >
          <h4 className="text-label-14 font-semibold text-gray-900">
            {t('timeline.inspector.observation.title')}
          </h4>

          {/* C1 — raw-JSON fallback: the stored observation member has a
              shape the structured editor cannot represent verbatim. The
              complete stored value seeds the textarea untouched (not dirty);
              the author edits it as JSON, and the write requires a valid
              JSON object. */}
          {form.observationRawText !== null ? (
            <div className="flex flex-col gap-1">
              <Label htmlFor="tl-observation-raw-json">
                {t('timeline.inspector.observation.rawLabel')}
              </Label>
              <Textarea
                id="tl-observation-raw-json"
                rows={6}
                className="font-mono text-copy-13-mono"
                value={form.observationRawText}
                onChange={(e) =>
                  setForm((prev) => ({ ...prev, observationRawText: e.target.value }))
                }
                placeholder={t('timeline.inspector.observation.rawPlaceholderJson')}
                spellCheck={false}
                data-testid="observation-raw-json"
              />
              {fieldErrors['observation.raw'] ? (
                <p className="text-copy-13 text-red-1000">
                  {fieldErrors['observation.raw']}
                </p>
              ) : null}
            </div>
          ) : (
            <>
          <div className="flex flex-col gap-1">
            <Label>{t('timeline.inspector.observation.observersLabel')}</Label>
            {observerOptions.length === 0 && rawSelectedObservers.length === 0 ? (
              <p className="text-copy-13 text-gray-700">
                {t('timeline.inspector.observation.noObserverOptions')}
              </p>
            ) : (
              <div className="flex flex-col gap-1">
                {observerOptions.map((nodeData) => {
                  const id = nodeData.key_block_id as string;
                  const name = nodeData.canonical_name;
                  const label =
                    name !== undefined && name !== id ? `${name} (${id})` : id;
                  return (
                    <label
                      key={id}
                      className="flex items-center gap-2 text-copy-13 text-gray-1000"
                    >
                      <input
                        type="checkbox"
                        data-testid={`observation-observer-${id}`}
                        checked={form.observers.includes(id)}
                        onChange={() => toggleObserver(id)}
                      />
                      {label}
                    </label>
                  );
                })}
                {rawSelectedObservers.map((id) => (
                  <label
                    key={id}
                    className="flex items-center gap-2 text-copy-13 text-gray-1000"
                  >
                    <input
                      type="checkbox"
                      data-testid={`observation-observer-${id}`}
                      checked
                      onChange={() => toggleObserver(id)}
                    />
                    {id}
                  </label>
                ))}
              </div>
            )}
            {fieldErrors['observation.observers'] ? (
              <p className="text-copy-13 text-red-1000">
                {fieldErrors['observation.observers']}
              </p>
            ) : null}
            {/* C3 — explicit PD-9 empty claim: distinct from an untouched
                unrecorded seed (key stays omitted). Clear records the
                author's deliberate "no observers" claim, so `observers: []`
                can be submitted even on the first save from a previously
                unrecorded event; the checkbox exposes the same claim when
                the selection is already empty. */}
            <label className="flex items-center gap-2 text-copy-13 text-gray-1000">
              <input
                type="checkbox"
                data-testid="observation-claim-none"
                checked={form.observersClaimNone}
                onChange={(e) =>
                  setForm((prev) => ({
                    ...prev,
                    observers: e.target.checked ? [] : prev.observers,
                    observersClaimNone: e.target.checked,
                  }))
                }
              />
              {t('timeline.inspector.observation.claimNone')}
            </label>
            <div className="flex items-center gap-2">
              <Input
                id="tl-observation-raw"
                value={rawObserver}
                onChange={(e) => setRawObserver(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    addRawObserver();
                  }
                }}
                placeholder={t('timeline.inspector.observation.rawPlaceholder')}
                aria-label={t('timeline.inspector.observation.rawPlaceholder')}
                data-testid="observation-raw-input"
              />
              <Button
                type="button"
                variant="tertiary"
                size="small"
                onClick={addRawObserver}
                disabled={rawObserver.trim().length === 0}
                data-testid="observation-raw-add"
              >
                {t('timeline.inspector.observation.addRaw')}
              </Button>
              <Button
                type="button"
                variant="tertiary"
                size="small"
                onClick={() =>
                  setForm((prev) => ({ ...prev, observers: [], observersClaimNone: true }))
                }
                disabled={form.observers.length === 0 && form.observersClaimNone}
                data-testid="observation-clear"
              >
                {t('timeline.inspector.observation.clear')}
              </Button>
            </div>
          </div>

          <div className="flex flex-col gap-1">
            <Label htmlFor="tl-observation-access">
              {t('timeline.inspector.observation.accessLabel')}
            </Label>
            <Textarea
              id="tl-observation-access"
              rows={3}
              className="font-mono text-copy-13-mono"
              value={form.accessText}
              onChange={(e) => setForm((prev) => ({ ...prev, accessText: e.target.value }))}
              placeholder={t('timeline.inspector.observation.accessPlaceholder')}
              spellCheck={false}
              data-testid="observation-access-input"
            />
            {fieldErrors['observation.access'] ? (
              <p className="text-copy-13 text-red-1000">
                {fieldErrors['observation.access']}
              </p>
            ) : null}
          </div>
            </>
          )}
        </section>
      ) : null}

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
          // QC1 W-001 (fix wave) — `data-work-id` must match the Work the
          // click ACTUALLY navigates to: the PD-7 winner when an event-level
          // bind exists, else the V1.123 surface-level fallback. In
          // multi-Work Worlds the two can differ — the DOM contract follows
          // the navigation target, not the surface fallback.
          data-work-id={ctx.boundWorkEventWorkId ?? ctx.boundWorkId}
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
