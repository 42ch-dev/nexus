/**
 * Work Timeline inspector — V1.123 P2 Task 6 (Moment inspector + Narrative
 * event inspector) + V1.123 P3 Task 4 (cross-surface navigation affordance).
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
 * V1.123 P3 Task 4 — Narrative event inspector ALSO surfaces a "View on World
 * Timeline" affordance when the active Work is bound to a World
 * (`WorkDetailResponse.world_id`). The affordance is reserved for the
 * Narrative-event binding axis (architect §3.4 — Moment-on-Outline carrier
 * has no Work-event → World-event binding today; scene/beat inspectors do NOT
 * receive this CTA). When the Work has no `world_id`, the affordance hides
 * (honest scope cut per plan §"If binding is missing or unreliable, P3 hides
 * the affordance").
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
import { Link } from 'react-router';
import { BookMarked, Flag, Globe, Hourglass, Milestone, Pencil } from 'lucide-react';
import type { Node } from '@xyflow/react';

import type { WorkTimelineNodeData } from './work-timeline-canvas-adapter';
import type { TimelineNodeData } from '../timeline-canvas/timeline-canvas-adapter';

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
  crossSurfaceAffordance,
}: {
  title: string;
  description: string;
  accent: 'worldkb' | 'outline';
  workId: string;
  children?: React.ReactNode;
  /**
   * Optional cross-surface navigation affordance rendered between the field
   * block and the Edit-in-Outline CTA (V1.123 P3 Task 4). The shell stays
   * agnostic to the affordance's destination; the caller (e.g. the event
   * inspector) composes the right element. Null/undefined when the
   * per-inspector binding is absent (honest scope cut).
   */
  crossSurfaceAffordance?: React.ReactNode;
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
      {crossSurfaceAffordance}
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

// ─── V1.123 P3 Task 4 — cross-surface affordance primitive ─────────────────

/**
 * "View on World Timeline" affordance — fired by the Narrative event
 * inspector when the Work is bound to a World.
 *
 * The orchestrator owns the navigation; this primitive only fires the
 * callback. Both `worldId` AND `onViewOnWorldTimeline` MUST be present for
 * the affordance to render (the orchestrator wires the callback only when
 * it has a valid World id and a real `useNavigate`). Honest scope cut:
 * either slot absent → CTA hidden, no silent degradation.
 *
 * The affordance is reserved for the Narrative-event binding axis (architect
 * §3.4 — Moment-on-Outline carrier has no Work-event → World-event binding
 * today). Scene + beat inspectors do NOT surface this CTA in V1.123.
 *
 * `simplify:` the per-inspector inlining keeps the dispatch contract obvious.
 * If P4 lands a generic inspector shell, this primitive is the upgrade seed.
 */
function ViewOnWorldTimelineAffordance({
  worldId,
  onViewOnWorldTimeline,
  node,
}: {
  worldId?: string;
  onViewOnWorldTimeline?: (node: Node<WorkTimelineNodeData>) => void;
  node: Node<WorkTimelineNodeData>;
}) {
  const { t } = useTranslation('canvas');
  if (!worldId || !onViewOnWorldTimeline) return null;
  return (
    <button
      type="button"
      data-testid="work-timeline-view-on-world-timeline"
      data-world-id={worldId}
      onClick={() => onViewOnWorldTimeline(node)}
      aria-label={t('workTimeline.inspector.viewOnWorldTimelineAria', {
        defaultValue: "Open this Work's bound World on the World Timeline",
      })}
      className="inline-flex items-center gap-1.5 self-start rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 shadow-elevation-2 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
    >
      <Globe className="h-3.5 w-3.5" aria-hidden />
      {t('workTimeline.inspector.viewOnWorldTimeline', {
        defaultValue: 'View on World Timeline',
      })}
    </button>
  );
}

// ─── Narrative event inspector ─────────────────────────────────────────────

/**
 * Narrative event inspector — Work-scoped event on the Narrative when-axis.
 * Shows event id + chapter anchor + description (if any) + the Edit-in-Outline
 * hand-off. Worldkb accent mirrors the Narrative event node.
 *
 * V1.123 P3 Task 4 — also surfaces "View on World Timeline" when the Work is
 * bound to a World (`worldId` supplied + `onViewOnWorldTimeline` wired by the
 * orchestrator). Honest scope cut: either slot absent → affordance hidden.
 */
export function WorkTimelineEventInspector({
  node,
  workId,
  worldId,
  onViewOnWorldTimeline,
}: {
  node: Node<WorkTimelineNodeData>;
  workId: string;
  /** Optional bound World id (P3 Task 4 cross-surface navigation). */
  worldId?: string;
  /**
   * Optional cross-surface navigation hand-off — fires when the user clicks
   * "View on World Timeline". The orchestrator composes the URL and calls
   * `useNavigate`; the inspector stays decoupled from routing.
   */
  onViewOnWorldTimeline?: (node: Node<WorkTimelineNodeData>) => void;
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
      crossSurfaceAffordance={
        <ViewOnWorldTimelineAffordance
          worldId={worldId}
          onViewOnWorldTimeline={onViewOnWorldTimeline}
          node={node}
        />
      }
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
 *
 * V1.123 P3 Task 4 — scene inspectors do NOT surface the cross-surface
 * affordance: the Work-event → World-event binding axis is Narrative-only
 * (architect §3.4 — Moment-on-Outline carrier has no World-event binding
 * today). The signature still accepts the slots for forward compatibility
 * (P4+ may revisit when the wire exposes scene-anchored World events).
 */
export function WorkTimelineMomentSceneInspector({
  node,
  workId,
  worldId: _worldId,
  onViewOnWorldTimeline: _onViewOnWorldTimeline,
}: {
  node: Node<WorkTimelineNodeData>;
  workId: string;
  /** Reserved for forward compatibility (P3 Task 4 — unused on Moment nodes). */
  worldId?: string;
  /** Reserved for forward compatibility (P3 Task 4 — unused on Moment nodes). */
  onViewOnWorldTimeline?: (node: Node<WorkTimelineNodeData>) => void;
}) {
  // Slots intentionally unused on Moment inspectors — see JSDoc above. They
  // exist on the signature so callers (e.g. tests checking that Moment nodes
  // do NOT surface the CTA even when the slots are wired) can pass them
  // without TypeScript errors. If P4+ extends the cross-surface binding to
  // scene-anchored World events, this is where the affordance would render.
  // The underscore-prefixed binds suppress `noUnusedParameters` without
  // adding runtime cost.
  void _worldId;
  void _onViewOnWorldTimeline;
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

// ─── V1.156 P2 T2 — read-only Brief-era inspector ──────────────────────────

/**
 * Work-Brief era inspector — read-only era detail for a `timeline-brief-era`
 * node selected on the Work Timeline Brief layer.
 *
 * Work-Brief is a read-only **projection** of the bound World's Brief
 * (PD-2): Brief is World spine; the Work does NOT gain an authored Brief
 * and there is NO Work-owned Brief write flow. Mirrors the World Timeline's
 * Brief-era inspector marker chrome (era id pill, time span, world summary,
 * version) but is strictly display-only:
 *   - NO title/body editors, NO Save, NO `kb.patch_entity` write path
 *     (P1 fix-wave lesson W-1 applied proactively — the World surface owns
 *     Brief writes via its own inspector; the Work surface never patches).
 *   - NO "Edit in Outline" CTA — the era is World-owned, not a Work
 *     manuscript node.
 *   - A "View on World Timeline" Link to the bound World's Brief layer
 *     (`/worlds/:worldId/timeline?layer=brief`) surfaces the source-World
 *     attribution (spec §3.3.3 — full bound-World Brief with source-World
 *     attribution). Hidden when no World is bound (honest scope cut).
 */
export function WorkTimelineBriefEraInspector({
  node,
  worldId,
}: {
  node: Node<WorkTimelineNodeData>;
  /** Optional bound World id — drives the "View on World Timeline" CTA. */
  worldId?: string;
}) {
  const { t } = useTranslation('canvas');
  // Brief-era nodes carry the World Timeline carrier (`TimelineNodeData`
  // with `layoutHint: 'brief'`) — Work-Brief reuses the World Brief
  // projection verbatim (T1). The Work surface never reads
  // `WorkTimelineNodeData` fields on Brief nodes; read the era markers
  // from the World carrier.
  const data = node.data as unknown as TimelineNodeData;
  const eraId = data.eraId;
  const startHint = data.startHint;
  const endHint = data.endHint;
  const worldSummary = data.worldSummary;

  // The time-span label mirrors the Brief-era node card: prefer
  // `start_hint → end_hint`; fall back to whichever hint exists; fall back
  // to the temporal-unknown label when neither is present (same format as
  // the World Timeline Brief-era inspector).
  const span = (() => {
    if (startHint && endHint) {
      return t('workTimeline.briefEraNode.span', { start: startHint, end: endHint });
    }
    if (startHint) return startHint;
    if (endHint) return endHint;
    return t('workTimeline.briefEraNode.temporalUnknown', {
      defaultValue: 'Era time span unknown',
    });
  })();

  return (
    <form
      data-testid="work-timeline-brief-era-inspector"
      aria-label={t('workTimeline.briefEraInspector.aria', {
        name: data.canonical_name,
        defaultValue: 'Brief-era inspector for {{name}}',
      })}
      className="flex flex-col gap-3"
    >
      <div className="flex items-center justify-between gap-2">
        <h3
          className="flex items-center gap-2 text-heading-14 font-heading font-semibold text-canvas-worldkb-accent"
          // eslint-disable-next-line react/forbid-dom-props
          data-testid="work-timeline-brief-era-inspector-title"
        >
          <Hourglass
            className="h-4 w-4 flex-shrink-0 text-canvas-worldkb-accent"
            aria-hidden
          />
          {t('workTimeline.briefEraInspector.title', { defaultValue: 'Brief era' })}
        </h3>
        {data.version !== undefined ? (
          <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
            {t('workTimeline.inspector.version', {
              version: data.version,
              defaultValue: `v${data.version}`,
            })}
          </span>
        ) : null}
      </div>
      <p className="text-copy-13 text-gray-700">
        {t('workTimeline.briefEraInspector.description', {
          defaultValue:
            'World-shape era marker from the bound World’s Brief. Read-only — Brief is World spine.',
        })}
      </p>

      {/* Era identity block — read-only marker fields surface the era's
          identity (mirrors the World Timeline Brief-era inspector chrome). */}
      <div
        className="flex flex-col gap-2 rounded-card border border-gray-alpha-400 bg-background-100 p-3"
        aria-label={t('workTimeline.briefEraInspector.identityAria', {
          defaultValue: 'Era identity markers',
        })}
      >
        {eraId ? (
          <div className="flex flex-col gap-1">
            <span className="text-label-12 font-semibold uppercase tracking-wide text-gray-700">
              {t('workTimeline.briefEraInspector.field.eraId', {
                defaultValue: 'Era id',
              })}
            </span>
            <span
              className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-900 self-start"
              // eslint-disable-next-line react/forbid-dom-props
              data-testid="work-timeline-brief-era-inspector-era-id"
            >
              {eraId}
            </span>
          </div>
        ) : null}

        <div className="flex flex-col gap-1">
          <span className="text-label-12 font-semibold uppercase tracking-wide text-gray-700">
            {t('workTimeline.briefEraInspector.field.span', {
              defaultValue: 'Time span',
            })}
          </span>
          <span
            className="rounded-pill border border-canvas-worldkb-accent/30 bg-canvas-worldkb-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-worldkb-accent self-start"
            // eslint-disable-next-line react/forbid-dom-props
            data-testid="work-timeline-brief-era-inspector-span"
          >
            {span}
          </span>
        </div>

        {worldSummary ? (
          <div className="flex flex-col gap-1">
            <span className="text-label-12 font-semibold uppercase tracking-wide text-gray-700">
              {t('workTimeline.briefEraInspector.field.worldSummary', {
                defaultValue: 'World summary',
              })}
            </span>
            <p
              className="text-copy-13 text-gray-900"
              // eslint-disable-next-line react/forbid-dom-props
              data-testid="work-timeline-brief-era-inspector-world-summary"
            >
              {worldSummary}
            </p>
          </div>
        ) : null}
      </div>

      {/* Read-only note (PD-2): the Work surface owns NO Brief write path.
          The bound World owns Brief authoring (World spine). */}
      <p className="text-copy-13 text-gray-700">
        {t('workTimeline.briefEraInspector.readOnly', {
          defaultValue: 'Read-only — Brief belongs to the bound World.',
        })}
      </p>

      {/* Cross-surface affordance — source-World attribution (spec §3.3.3:
          full bound-World Brief with source-World attribution). Direct Link
          mirrors the Edit-in-Outline Link pattern; targets the bound World's
          Timeline Brief layer. Hidden when no World is bound. */}
      {worldId ? (
        <Link
          to={`/worlds/${encodeURIComponent(worldId)}/timeline?layer=brief`}
          data-testid="work-timeline-brief-era-view-on-world-timeline"
          data-world-id={worldId}
          className="inline-flex items-center gap-1.5 self-start rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 shadow-elevation-2 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
        >
          <Globe className="h-3.5 w-3.5" aria-hidden />
          {t('workTimeline.inspector.viewOnWorldTimeline', {
            defaultValue: 'View on World Timeline',
          })}
        </Link>
      ) : null}
    </form>
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
 * (read-only in V1.123): every branch renders read-only details; the
 * Narrative/Moment branches also add the Edit-in-Outline CTA while the
 * Brief-era branch (PD-2 — read-only projection) is display-only without
 * it; no write is invoked from the Work Timeline surface.
 *
 * V1.123 P3 Task 4 — the dispatcher now carries the cross-surface navigation
 * slots (`worldId` + `onViewOnWorldTimeline`) so the Narrative event
 * inspector can render the "View on World Timeline" affordance when the
 * orchestrator wires them.
 *
 * V1.156 P2 T2 — Brief-era nodes (`type === 'timeline-brief-era'`) carry the
 * World Timeline carrier (`TimelineNodeData` with `layoutHint: 'brief'`),
 * NOT `WorkTimelineNodeData` — dispatch on the registered node type FIRST so
 * Brief-era selections surface the read-only Brief-era inspector (PD-2 —
 * no write path from Brief nodes; P1 fix-wave lesson W-1 applied
 * proactively).
 */
export function renderWorkTimelineInspector(
  node: Node<WorkTimelineNodeData>,
  workId: string,
  crossSurface?: {
    worldId?: string;
    onViewOnWorldTimeline?: (node: Node<WorkTimelineNodeData>) => void;
  },
): React.ReactNode {
  const data = node.data;
  if (node.type === 'timeline-brief-era') {
    return (
      <WorkTimelineBriefEraInspector
        node={node}
        worldId={crossSurface?.worldId}
      />
    );
  }
  if (data.nodeKind === 'event') {
    return (
      <WorkTimelineEventInspector
        node={node}
        workId={workId}
        worldId={crossSurface?.worldId}
        onViewOnWorldTimeline={crossSurface?.onViewOnWorldTimeline}
      />
    );
  }
  if (data.nodeKind === 'scene') {
    return <WorkTimelineMomentSceneInspector node={node} workId={workId} />;
  }
  if (data.nodeKind === 'beat') {
    return <WorkTimelineMomentBeatInspector node={node} workId={workId} />;
  }
  return null;
}
