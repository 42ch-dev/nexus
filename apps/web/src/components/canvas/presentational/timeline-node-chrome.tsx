/**
 * Timeline node body chrome — pure presentational extracts for World Timeline
 * and Work Timeline node cards (V1.124 P0 T2).
 *
 * Pair with `NodeChromeShell` for the outer card. This module owns the **body**
 * only (icon + title + badge row + optional summary/meta). RF wrappers stay
 * App-local (handles, selected/dragging, i18n resolution, wire labels).
 *
 * Presentational boundary (architect contract,
 * `.mstar/iterations/v1.124/specs/studio-timeline-fixture-boundaries.md`):
 *   No React Flow package, no wire-contract package, no i18n hook, no
 *   react-router, no NexusClient, no app providers. Props are resolved strings
 *   (and plain numbers) only — Studio fixtures pass static English product
 *   vocabulary; App RF wrappers resolve `t()` before calling.
 *
 * Layer-accent migration (V1.123 P4 completion inside the shared extract):
 *   Event + Work Narrative badges use `canvas-layer-narrative-accent`
 *   (not `canvas-worldkb-accent`). Brief uses `canvas-layer-brief-accent`;
 *   Moment scene/beat use `canvas-layer-moment-accent`.
 *
 * Studio import: `@web-canvas/timeline-node-chrome` (existing `@web-canvas/*`
 * alias — no new Vite root).
 */
import { BookMarked, Flag, Hourglass, Milestone } from 'lucide-react';

// ─── World Timeline — Brief-era ─────────────────────────────────────────────

export interface TimelineBriefEraChromeProps {
  /** Canonical era name (already resolved; wrapper applies unnamed fallback). */
  title: string;
  /** Block-type pill label (e.g. "Era") — plain string, no contracts. */
  blockTypeLabel: string;
  /**
   * Resolved time-span text (`start → end`, start-only, or end-only).
   * `null` → render the temporal-unknown pill with `temporalUnknownLabel`.
   */
  timeSpan: string | null;
  /** Resolved temporal-unknown pill copy (i18n stays in the RF wrapper). */
  temporalUnknownLabel: string;
  /** Optional era id pill. */
  eraId?: string;
  /** Optional world-summary line (clamped). */
  worldSummary?: string;
  /**
   * Resolved source-anchor count label (e.g. "3 source anchors").
   * Version suffix is appended by this chrome as ` · v{version}`.
   */
  sourceAnchorLabel: string;
  version: number;
}

/**
 * Brief-era marker body — Hourglass icon + title + block-type / time-span /
 * era-id badges + optional world summary + source meta.
 * Layer accent: `--color-canvas-layer-brief-accent` (icon + time-span badge).
 */
export function TimelineBriefEraChrome({
  title,
  blockTypeLabel,
  timeSpan,
  temporalUnknownLabel,
  eraId,
  worldSummary,
  sourceAnchorLabel,
  version,
}: TimelineBriefEraChromeProps) {
  return (
    <>
      <div className="flex items-center gap-2">
        <Hourglass
          className="h-4 w-4 flex-shrink-0 text-canvas-layer-brief-accent"
          aria-hidden
        />
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={title}
        >
          {title}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {blockTypeLabel}
        </span>
        {timeSpan ? (
          <span className="rounded-pill border border-canvas-layer-brief-accent/30 bg-canvas-layer-brief-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-layer-brief-accent">
            {timeSpan}
          </span>
        ) : (
          <span className="rounded-pill border border-gray-alpha-400 bg-gray-alpha-100 px-1.5 py-0.5 text-label-12 text-gray-700">
            {temporalUnknownLabel}
          </span>
        )}
        {eraId ? (
          <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
            {eraId}
          </span>
        ) : null}
      </div>
      {worldSummary ? (
        <p
          className="mt-1 line-clamp-2 text-label-12 text-gray-700"
          title={worldSummary}
        >
          {worldSummary}
        </p>
      ) : null}
      <p className="mt-1 text-label-12 text-gray-700">
        {sourceAnchorLabel} · v{version}
      </p>
    </>
  );
}

// ─── World Timeline — Event ─────────────────────────────────────────────────

export interface TimelineEventChromeProps {
  title: string;
  blockTypeLabel: string;
  /** Resolved occurred-at hint, or null → temporal-unknown pill. */
  occurredAtHint: string | null;
  temporalUnknownLabel: string;
  sourceAnchorLabel: string;
  version: number;
}

/**
 * Narrative event body — title + block-type / occurred-at badges + source meta.
 * Layer accent: `--color-canvas-layer-narrative-accent` on the dated badge
 * (V1.123 P4 / V1.124 extract migration off worldkb-accent).
 */
export function TimelineEventChrome({
  title,
  blockTypeLabel,
  occurredAtHint,
  temporalUnknownLabel,
  sourceAnchorLabel,
  version,
}: TimelineEventChromeProps) {
  return (
    <>
      <div className="flex items-center justify-between gap-2">
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={title}
        >
          {title}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {blockTypeLabel}
        </span>
        {occurredAtHint ? (
          <span className="rounded-pill border border-canvas-layer-narrative-accent/30 bg-canvas-layer-narrative-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-layer-narrative-accent">
            {occurredAtHint}
          </span>
        ) : (
          <span className="rounded-pill border border-gray-alpha-400 bg-gray-alpha-100 px-1.5 py-0.5 text-label-12 text-gray-700">
            {temporalUnknownLabel}
          </span>
        )}
      </div>
      <p className="mt-1 text-label-12 text-gray-700">
        {sourceAnchorLabel} · v{version}
      </p>
    </>
  );
}

// ─── World Timeline — KeyBlock Context cluster ──────────────────────────────

export interface TimelineKeyBlockChromeProps {
  title: string;
  blockTypeLabel: string;
  sourceAnchorLabel: string;
  version: number;
}

/**
 * Context-cluster KeyBlock body — title + block-type pill + source meta.
 * No dedicated layer badge (distinct by absence of temporal/era chrome).
 */
export function TimelineKeyBlockChrome({
  title,
  blockTypeLabel,
  sourceAnchorLabel,
  version,
}: TimelineKeyBlockChromeProps) {
  return (
    <>
      <div className="flex items-center justify-between gap-2">
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={title}
        >
          {title}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {blockTypeLabel}
        </span>
      </div>
      <p className="mt-1 text-label-12 text-gray-700">
        {sourceAnchorLabel} · v{version}
      </p>
    </>
  );
}

// ─── Work Timeline — Narrative event ────────────────────────────────────────

export interface WorkTimelineNarrativeEventChromeProps {
  title: string;
  eventId: string;
  /**
   * Resolved chapter-anchor badge text (e.g. "Ch. 3"), or null →
   * `noChapterLabel` pill.
   */
  chapterAnchor: string | null;
  /** Resolved "No chapter anchor" (or locale equivalent). */
  noChapterLabel: string;
  description?: string;
}

/**
 * Work Narrative event body — Flag icon + title + event-id / chapter badges
 * + optional description.
 * Layer accent: `--color-canvas-layer-narrative-accent` on Flag + chapter badge
 * (V1.123 P4 / V1.124 extract migration off worldkb-accent).
 */
export function WorkTimelineNarrativeEventChrome({
  title,
  eventId,
  chapterAnchor,
  noChapterLabel,
  description,
}: WorkTimelineNarrativeEventChromeProps) {
  return (
    <>
      <div className="flex items-center gap-2">
        <Flag
          className="h-4 w-4 flex-shrink-0 text-canvas-layer-narrative-accent"
          aria-hidden
        />
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={title}
        >
          {title}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {eventId}
        </span>
        {chapterAnchor !== null ? (
          <span className="rounded-pill border border-canvas-layer-narrative-accent/30 bg-canvas-layer-narrative-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-layer-narrative-accent">
            {chapterAnchor}
          </span>
        ) : (
          <span className="rounded-pill border border-gray-alpha-400 bg-gray-alpha-100 px-1.5 py-0.5 text-label-12 text-gray-700">
            {noChapterLabel}
          </span>
        )}
      </div>
      {description ? (
        <p
          className="mt-1 line-clamp-2 text-label-12 text-gray-700"
          title={description}
        >
          {description}
        </p>
      ) : null}
    </>
  );
}

// ─── Work Timeline — Moment scene ───────────────────────────────────────────

export interface WorkTimelineMomentSceneChromeProps {
  title: string;
  sceneId: string;
  /** Resolved manuscript-anchor badge (e.g. "Ch. 1 · sc-1"), or null. */
  manuscriptAnchorLabel: string | null;
  status?: string;
}

/**
 * Moment scene body — BookMarked icon + title + scene-id / manuscript-anchor
 * / optional status badges.
 * Layer accent: `--color-canvas-layer-moment-accent` (icon + anchor badge).
 */
export function WorkTimelineMomentSceneChrome({
  title,
  sceneId,
  manuscriptAnchorLabel,
  status,
}: WorkTimelineMomentSceneChromeProps) {
  return (
    <>
      <div className="flex items-center gap-2">
        <BookMarked
          className="h-4 w-4 flex-shrink-0 text-canvas-layer-moment-accent"
          aria-hidden
        />
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={title}
        >
          {title}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {sceneId}
        </span>
        {manuscriptAnchorLabel ? (
          <span className="rounded-pill border border-canvas-layer-moment-accent/30 bg-canvas-layer-moment-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-layer-moment-accent">
            {manuscriptAnchorLabel}
          </span>
        ) : null}
        {status ? (
          <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 text-label-12 text-gray-700">
            {status}
          </span>
        ) : null}
      </div>
    </>
  );
}

// ─── Work Timeline — Moment beat ────────────────────────────────────────────

export interface WorkTimelineMomentBeatChromeProps {
  title: string;
  /** Resolved manuscript-anchor badge, or null. */
  manuscriptAnchorLabel: string | null;
  status?: string;
}

/**
 * Moment beat body — Milestone icon + title + manuscript-anchor / optional
 * status badges.
 * Layer accent: `--color-canvas-layer-moment-accent`.
 */
export function WorkTimelineMomentBeatChrome({
  title,
  manuscriptAnchorLabel,
  status,
}: WorkTimelineMomentBeatChromeProps) {
  return (
    <>
      <div className="flex items-center gap-2">
        <Milestone
          className="h-3.5 w-3.5 flex-shrink-0 text-canvas-layer-moment-accent"
          aria-hidden
        />
        <span
          className="truncate font-heading text-copy-13 font-medium text-gray-1000"
          title={title}
        >
          {title}
        </span>
      </div>
      <div className="mt-0.5 flex flex-wrap items-center gap-1">
        {manuscriptAnchorLabel ? (
          <span className="rounded-pill border border-canvas-layer-moment-accent/30 bg-canvas-layer-moment-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-layer-moment-accent">
            {manuscriptAnchorLabel}
          </span>
        ) : null}
        {status ? (
          <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 text-label-12 text-gray-700">
            {status}
          </span>
        ) : null}
      </div>
    </>
  );
}
