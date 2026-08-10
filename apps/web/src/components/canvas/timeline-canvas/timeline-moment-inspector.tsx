/**
 * World Timeline Moment inspector — V1.156 P1 fix-wave 1 (F1).
 *
 * Read-only inspector for Moment scene/beat nodes projected onto the World
 * Timeline Moment layer (`projectMomentLayer` — fixture-driven
 * read/projection of bound Works' Scene/Beat data).
 *
 * Two inspector shapes dispatch on `node.data.nodeKind`, mirroring the Work
 * Timeline `renderWorkTimelineInspector` dispatch (V1.123):
 *   • `scene` → `TimelineMomentSceneInspector`
 *   • `beat`  → `TimelineMomentBeatInspector`
 *
 * Read-only per PD-3 (World Timeline Moment is a READ/projection layer —
 * Moments remain Work-owned; no World Moment authoring flow) + V1.123
 * layer-feel parity with Work-Moment (§2.4 — World-Moment ≡ Work-Moment:
 * both selectable, both read-only inspector). The inspector surfaces the
 * manuscript anchor + identity + status but renders NO Save / NO
 * `kb.patch_entity` path. The generic KB `TimelineInspector` (title + body
 * JSON editor wired to `ctxRef.onPatchEntity`) MUST NOT be reachable from
 * Moment nodes: their `WorkTimelineNodeData` carrier has no `key_block_id`,
 * so a Save would fire `kb.patch_entity` with `entity_id: undefined` — a
 * guaranteed-failing write request on a read layer (PD-3 violation).
 *
 * No "Edit in Outline" CTA: the Work Timeline moment inspectors link to
 * `/works/:workId/outline`, but on the World surface the node's `workId`
 * field carries the WORLD id (the graph has no per-Work attribution until
 * DR-26) — rendering that CTA would navigate to `/works/<worldId>/outline`,
 * a wrong destination (qc3 F-4 / qc2 M-1 footgun). Honest scope cut:
 * read-only fields + copy stating edits route through the bound Work's
 * Outline surface.
 */
import { useTranslation } from 'react-i18next';
import type { ReactNode } from 'react';
import { BookMarked, Milestone } from 'lucide-react';
import type { Node } from '@xyflow/react';

import type { WorkTimelineNodeData } from '../work-timeline-canvas/work-timeline-canvas-adapter';

// ─── Field row primitive (mirrors work-timeline-inspector.tsx) ─────────────

function FieldRow({
  label,
  value,
}: {
  label: string;
  value: string | number | null | undefined;
}) {
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
        {t('timeline.moment.inspector.noManuscriptAnchor', {
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
        {t('timeline.moment.inspector.manuscriptAnchor', {
          defaultValue: 'Manuscript anchor',
        })}
      </dt>
      <dd className="text-copy-13-mono text-gray-1000">{parts.join(' · ')}</dd>
    </div>
  );
}

// ─── Shared read-only shell ────────────────────────────────────────────────

/**
 * Read-only inspector shell — title + description + children + a read-only
 * note. Outline accent (amber-700) mirrors the Work Timeline Moment
 * inspectors + the Work-scoped Moment node feel (V1.123 layer-feel §2.4).
 * NO submit wiring, NO Save, NO cross-surface CTA — display-only.
 */
function TimelineMomentInspectorShell({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children?: ReactNode;
}) {
  const { t } = useTranslation('canvas');
  return (
    <form
      data-testid="timeline-moment-inspector"
      aria-label={t('timeline.moment.inspector.aria', {
        name: title,
        defaultValue: 'World Timeline Moment inspector for {{name}}',
      })}
      className="flex flex-col gap-3"
    >
      <div className="flex flex-col gap-1">
        <h3 className="text-heading-14 font-heading font-semibold text-canvas-outline-accent">
          {title}
        </h3>
        <p className="text-copy-13 text-gray-700">{description}</p>
      </div>
      {children}
      <p className="text-copy-13 text-gray-700">
        {t('timeline.moment.inspector.readOnly', {
          defaultValue: 'Read-only — Moments are owned by bound Works.',
        })}
      </p>
    </form>
  );
}

// ─── Moment scene inspector ────────────────────────────────────────────────

/**
 * Moment scene inspector — scene card detail. Shows scene id + chapter +
 * status (if any) + the chapter/scene manuscript anchor. Outline accent
 * mirrors the Moment scene node (Work-scoped outline-projection). Read-only:
 * no Save, no `kb.patch_entity` path, no Edit-in-Outline CTA (the World
 * surface cannot resolve the owning Work id — see module doc).
 */
export function TimelineMomentSceneInspector({
  node,
}: {
  node: Node<WorkTimelineNodeData>;
}) {
  const { t } = useTranslation('canvas');
  const d = node.data;
  return (
    <TimelineMomentInspectorShell
      title={
        d.label ||
        t('timeline.moment.inspector.unnamedScene', {
          defaultValue: '(untitled scene)',
        })
      }
      description={t('timeline.moment.inspector.sceneDescription', {
        defaultValue:
          'Scene-level manuscript anchor on the Moment layer. Moments are owned by bound Works; edits route through the Outline surface.',
      })}
    >
      <div className="flex items-start gap-2">
        <BookMarked
          className="mt-0.5 h-4 w-4 flex-shrink-0 text-canvas-outline-accent"
          aria-hidden
        />
        <dl className="flex flex-col gap-2">
          <FieldRow
            label={t('timeline.moment.inspector.fields.sceneId', {
              defaultValue: 'Scene id',
            })}
            value={d.sceneId}
          />
          <FieldRow
            label={t('timeline.moment.inspector.fields.chapter', {
              defaultValue: 'Chapter',
            })}
            value={d.realizesChapterId}
          />
          <FieldRow
            label={t('timeline.moment.inspector.fields.status', {
              defaultValue: 'Status',
            })}
            value={d.status ?? null}
          />
          <ManuscriptAnchorBlock anchor={d.manuscriptAnchor} />
        </dl>
      </div>
    </TimelineMomentInspectorShell>
  );
}

// ─── Moment beat inspector ─────────────────────────────────────────────────

/**
 * Moment beat inspector — beat pin detail. Shows beat id + parent scene id +
 * status (if any) + the chapter/scene/beat manuscript anchor. Outline accent
 * mirrors the Moment beat node. Read-only: no Save, no `kb.patch_entity`
 * path, no Edit-in-Outline CTA (see module doc).
 */
export function TimelineMomentBeatInspector({
  node,
}: {
  node: Node<WorkTimelineNodeData>;
}) {
  const { t } = useTranslation('canvas');
  const d = node.data;
  return (
    <TimelineMomentInspectorShell
      title={
        d.label ||
        t('timeline.moment.inspector.unnamedBeat', {
          defaultValue: '(untitled beat)',
        })
      }
      description={t('timeline.moment.inspector.beatDescription', {
        defaultValue:
          'Beat pin inside a scene. Moments are owned by bound Works; edits route through the Outline surface.',
      })}
    >
      <div className="flex items-start gap-2">
        <Milestone
          className="mt-0.5 h-3.5 w-3.5 flex-shrink-0 text-canvas-outline-accent"
          aria-hidden
        />
        <dl className="flex flex-col gap-2">
          <FieldRow
            label={t('timeline.moment.inspector.fields.beatId', {
              defaultValue: 'Beat id',
            })}
            value={d.beatId}
          />
          <FieldRow
            label={t('timeline.moment.inspector.fields.sceneId', {
              defaultValue: 'Scene id',
            })}
            value={d.sceneId}
          />
          <FieldRow
            label={t('timeline.moment.inspector.fields.status', {
              defaultValue: 'Status',
            })}
            value={d.status ?? null}
          />
          <ManuscriptAnchorBlock anchor={d.manuscriptAnchor} />
        </dl>
      </div>
    </TimelineMomentInspectorShell>
  );
}

// ─── Dispatch ──────────────────────────────────────────────────────────────

/**
 * Dispatch the selected node to the right World Timeline Moment inspector by
 * `node.data.nodeKind`. Returns `null` for unknown kinds so the
 * `useCanvasSurface` derived `surface.inspector` slot renders nothing when
 * the selection does not match a Moment node.
 *
 * Mirrors the Work Timeline `renderWorkTimelineInspector` dispatch (V1.123).
 * Every branch renders read-only details; no write is invoked from the World
 * Timeline Moment layer (PD-3).
 */
export function renderTimelineMomentInspector(
  node: Node<WorkTimelineNodeData>,
): ReactNode {
  const data = node.data;
  if (data.nodeKind === 'scene') {
    return <TimelineMomentSceneInspector node={node} />;
  }
  if (data.nodeKind === 'beat') {
    return <TimelineMomentBeatInspector node={node} />;
  }
  return null;
}
