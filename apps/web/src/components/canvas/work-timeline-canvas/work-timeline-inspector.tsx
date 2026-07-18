/**
 * Work Timeline inspector — V1.123 P2 Task 6 (Moment inspector + Narrative
 * event inspector).
 *
 * Three inspector shapes dispatch on `node.data.nodeKind`:
 *   • `event`  → `WorkTimelineEventInspector`     (Narrative layer)
 *   • `scene`  → `WorkTimelineMomentSceneInspector` (Moment layer)
 *   • `beat`   → `WorkTimelineMomentBeatInspector`  (Moment layer)
 *
 * Architect §6 — read-only in V1.123: every inspector surfaces the node's
 * manuscript anchor + identity + an "Edit in Outline" hand-off CTA. The CTA
 * is a `<Link>` to `/works/:workId/outline` (the Outline surface owns the
 * writes); no `patchOutline*` / `patchTimelineEvent` is wired from the Work
 * Timeline inspector.
 *
 * Visual differentiation (layer-feel §2.4):
 *   - Event inspector uses the worldkb accent spine (teal-700) — same as the
 *     Narrative event node (Timeline-family continuity).
 *   - Scene + beat inspectors use the outline accent spine (amber-700) —
 *     mirrors the Moment scene/beat nodes (Work-scoped outline-projection).
 *
 * `simplify:` the three inspectors share enough shape that a future iteration
 * MAY promote a shared `WorkTimelineInspectorShell`. V1.123 ships three leaf
 * components to keep the dispatch contract obvious and the inspector per-node
 * copy distinct. P4 may consolidate when the design system lands a generic
 * inspector shell.
 */
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import { BookMarked, Flag, Milestone, Pencil } from 'lucide-react';
import type { Node } from '@xyflow/react';

import type { WorkTimelineNodeData } from './work-timeline-canvas-adapter';

// ─── Shared shell ──────────────────────────────────────────────────────────

/**
 * Inspector shell — title + description + manuscript-anchor block + "Edit in
 * Outline" CTA. Accent (worldkb vs outline) discriminates Narrative vs Moment
 * feel at a glance.
 *
 * `simplify:` shared leaf shell rather than a `packages/nexus-ui` promotion.
 * The inspector title/description/anchor block are Work-Timeline-surface-
 * specific; P4 may revisit once the design system lands a generic inspector.
 */
function WorkTimelineInspectorShell({
  title,
  description,
  accent,
  workId,
  children,
}: {
  title: string;
  description: string;
  accent: 'worldkb' | 'outline';
  workId: string;
  children?: React.ReactNode;
}) {
  const { t } = useTranslation('canvas');
  const accentColor =
    accent === 'worldkb' ? 'text-canvas-worldkb-accent' : 'text-canvas-outline-accent';
  return (
    <form
      data-testid="work-timeline-inspector"
      aria-label={t('workTimeline.inspector.aria', {
        name: title,
        defaultValue: 'Work Timeline inspector for {{name}}',
      })}
      className="flex flex-col gap-3"
    >
      <div className="flex flex-col gap-1">
        <h3
          className={`text-heading-14 font-heading font-semibold ${accentColor}`}
        >
          {title}
        </h3>
        <p className="text-copy-13 text-gray-700">{description}</p>
      </div>
      {children}
      {/* "Edit in Outline" hand-off (architect §6 read-only invariant).
          The CTA navigates to the Outline surface where the writes live;
          no patch is invoked from this inspector. */}
      <Link
        to={`/works/${encodeURIComponent(workId)}/outline`}
        data-testid="work-timeline-inspector-edit-in-outline"
        className="inline-flex items-center gap-1.5 self-start rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 shadow-elevation-2 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
      >
        <Pencil className="h-3.5 w-3.5" aria-hidden />
        {t('workTimeline.inspector.editInOutline', { defaultValue: 'Edit in Outline' })}
      </Link>
    </form>
  );
}

// ─── Field row primitive ───────────────────────────────────────────────────

function FieldRow({ label, value }: { label: string; value: string | number | null | undefined }) {
  if (value === null || value === undefined || value === '') return null;
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-label-12 text-gray-700">{label}</dt>
      <dd className="text-copy-13-mono text-gray-1000">{value}</dd>
    </div>
  );
}

function ManuscriptAnchorBlock({
  anchor,
}: {
  anchor: { chapterId: number; sceneId?: string; beatId?: string } | undefined;
}) {
  const { t } = useTranslation('canvas');
  if (!anchor) {
    return (
      <p className="text-copy-13 text-gray-700">
        {t('workTimeline.inspector.noManuscriptAnchor', {
          defaultValue: 'No manuscript anchor',
        })}
      </p>
    );
  }
  const parts: string[] = [
    `Ch. ${anchor.chapterId}`,
    ...(anchor.sceneId ? [anchor.sceneId] : []),
    ...(anchor.beatId ? [anchor.beatId] : []),
  ];
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-label-12 text-gray-700">
        {t('workTimeline.inspector.manuscriptAnchor', { defaultValue: 'Manuscript anchor' })}
      </dt>
      <dd className="text-copy-13-mono text-gray-1000">{parts.join(' · ')}</dd>
    </div>
  );
}

// ─── Narrative event inspector ─────────────────────────────────────────────

/**
 * Narrative event inspector — Work-scoped event on the Narrative when-axis.
 * Shows event id + chapter anchor + description (if any) + the Edit-in-Outline
 * hand-off. Worldkb accent mirrors the Narrative event node.
 */
export function WorkTimelineEventInspector({
  node,
  workId,
}: {
  node: Node<WorkTimelineNodeData>;
  workId: string;
}) {
  const { t } = useTranslation('canvas');
  const d = node.data;
  return (
    <WorkTimelineInspectorShell
      title={d.label || t('workTimeline.narrativeEventNode.unnamed', { defaultValue: '(unnamed event)' })}
      description={t('workTimeline.inspector.eventDescription', {
        defaultValue:
          'Work-scoped event on the Narrative when-axis. Edits route through the Outline surface.',
      })}
      accent="worldkb"
      workId={workId}
    >
      <div className="flex items-start gap-2">
        <Flag className="mt-0.5 h-4 w-4 flex-shrink-0 text-canvas-worldkb-accent" aria-hidden />
        <dl className="flex flex-col gap-2">
          <FieldRow
            label={t('workTimeline.inspector.fields.eventId', { defaultValue: 'Event id' })}
            value={d.eventId}
          />
          <FieldRow
            label={t('workTimeline.inspector.fields.chapter', { defaultValue: 'Chapter' })}
            value={d.realizesChapterId}
          />
          <FieldRow
            label={t('workTimeline.inspector.fields.description', { defaultValue: 'Description' })}
            value={d.description}
          />
          <ManuscriptAnchorBlock anchor={d.manuscriptAnchor} />
        </dl>
      </div>
    </WorkTimelineInspectorShell>
  );
}

// ─── Moment scene inspector ────────────────────────────────────────────────

/**
 * Moment scene inspector — scene card detail. Shows scene id + chapter +
 * status (if any) + the manuscript anchor + Edit-in-Outline hand-off.
 * Outline accent mirrors the Moment scene node (Work-scoped outline-projection).
 */
export function WorkTimelineMomentSceneInspector({
  node,
  workId,
}: {
  node: Node<WorkTimelineNodeData>;
  workId: string;
}) {
  const { t } = useTranslation('canvas');
  const d = node.data;
  return (
    <WorkTimelineInspectorShell
      title={d.label || t('workTimeline.momentSceneNode.unnamed', { defaultValue: '(untitled scene)' })}
      description={t('workTimeline.inspector.sceneDescription', {
        defaultValue:
          'Scene-level manuscript anchor on the Moment layer. Edits route through the Outline surface.',
      })}
      accent="outline"
      workId={workId}
    >
      <div className="flex items-start gap-2">
        <BookMarked className="mt-0.5 h-4 w-4 flex-shrink-0 text-canvas-outline-accent" aria-hidden />
        <dl className="flex flex-col gap-2">
          <FieldRow
            label={t('workTimeline.inspector.fields.sceneId', { defaultValue: 'Scene id' })}
            value={d.sceneId}
          />
          <FieldRow
            label={t('workTimeline.inspector.fields.chapter', { defaultValue: 'Chapter' })}
            value={d.realizesChapterId}
          />
          <FieldRow
            label={t('workTimeline.inspector.fields.status', { defaultValue: 'Status' })}
            value={d.status ?? null}
          />
          <ManuscriptAnchorBlock anchor={d.manuscriptAnchor} />
        </dl>
      </div>
    </WorkTimelineInspectorShell>
  );
}

// ─── Moment beat inspector ─────────────────────────────────────────────────

/**
 * Moment beat inspector — beat pin detail. Shows beat id + parent scene id +
 * status (if any) + the chapter/scene/beat manuscript anchor + Edit-in-Outline
 * hand-off. Outline accent mirrors the Moment beat node.
 */
export function WorkTimelineMomentBeatInspector({
  node,
  workId,
}: {
  node: Node<WorkTimelineNodeData>;
  workId: string;
}) {
  const { t } = useTranslation('canvas');
  const d = node.data;
  return (
    <WorkTimelineInspectorShell
      title={d.label || t('workTimeline.momentBeatNode.unnamed', { defaultValue: '(untitled beat)' })}
      description={t('workTimeline.inspector.beatDescription', {
        defaultValue: 'Beat pin inside a scene. Edits route through the Outline surface.',
      })}
      accent="outline"
      workId={workId}
    >
      <div className="flex items-start gap-2">
        <Milestone className="mt-0.5 h-3.5 w-3.5 flex-shrink-0 text-canvas-outline-accent" aria-hidden />
        <dl className="flex flex-col gap-2">
          <FieldRow
            label={t('workTimeline.inspector.fields.beatId', { defaultValue: 'Beat id' })}
            value={d.beatId}
          />
          <FieldRow
            label={t('workTimeline.inspector.fields.sceneId', { defaultValue: 'Scene id' })}
            value={d.sceneId}
          />
          <FieldRow
            label={t('workTimeline.inspector.fields.status', { defaultValue: 'Status' })}
            value={d.status ?? null}
          />
          <ManuscriptAnchorBlock anchor={d.manuscriptAnchor} />
        </dl>
      </div>
    </WorkTimelineInspectorShell>
  );
}

// ─── Dispatch ──────────────────────────────────────────────────────────────

/**
 * Dispatch the selected node to the right Work Timeline inspector by
 * `node.data.nodeKind`. Returns `null` for unknown kinds so the
 * `useCanvasSurface` derived `surface.inspector` slot renders nothing when
 * the selection does not match a Work Timeline node.
 *
 * The dispatch mirrors the V1.123 P1 Timeline adapter's
 * `renderInspector` dispatch (Brief-era vs Narrative event). Architect §6
 * (read-only in V1.123): every branch renders read-only details + the
 * Edit-in-Outline CTA; no write is invoked from the Work Timeline surface.
 */
export function renderWorkTimelineInspector(
  node: Node<WorkTimelineNodeData>,
  workId: string,
): React.ReactNode {
  const data = node.data;
  if (data.nodeKind === 'event') {
    return <WorkTimelineEventInspector node={node} workId={workId} />;
  }
  if (data.nodeKind === 'scene') {
    return <WorkTimelineMomentSceneInspector node={node} workId={workId} />;
  }
  if (data.nodeKind === 'beat') {
    return <WorkTimelineMomentBeatInspector node={node} workId={workId} />;
  }
  return null;
}
