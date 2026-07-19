/**
 * Studio fixtures for Layer breadcrumb (V1.124 P2 Task 3a).
 *
 * Composes `@web-canvas/layer-breadcrumb` — the same presentational extract
 * World Timeline and Work Timeline headers use after resolving i18n labels.
 *
 * Boundary: no RF, no daemon, no contracts, no `useTranslation`.
 * Matrix from surface-audit-checklist.md §4.2 / layer-feel §3.4.
 */
import { useState, type ReactNode } from 'react';

import { LayerBreadcrumb } from '@web-canvas/layer-breadcrumb'; // @web-canvas/layer-breadcrumb - transitional until package promotion criteria met

/* ------------------------------------------------------------------ */
/*  Shared fixture frame                                                */
/* ------------------------------------------------------------------ */

function FixtureFrame({
  title,
  description,
  testId,
  children,
}: {
  title: string;
  description: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <div
      className="mb-8 rounded-card border border-gray-alpha-200 bg-background-100 p-4"
      data-testid={testId}
    >
      <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">{title}</h4>
      <p className="text-copy-13 text-gray-700 mb-4">{description}</p>
      {children}
    </div>
  );
}

function VariantChip({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-label-12 font-medium text-gray-500">{label}</span>
      <div className="rounded-card border border-gray-alpha-200 bg-canvas-surface px-3 py-2">
        {children}
      </div>
    </div>
  );
}

function VariantMatrix({
  testId,
  children,
}: {
  testId: string;
  children: ReactNode;
}) {
  return (
    <div
      className="flex flex-wrap gap-6 rounded-card bg-canvas-surface p-6"
      data-testid={testId}
    >
      {children}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  World Timeline — Brief ↔ Narrative                                  */
/* ------------------------------------------------------------------ */

type WorldLayer = 'brief' | 'narrative';

function WorldBreadcrumbMatrix() {
  const [active, setActive] = useState<WorldLayer>('narrative');

  return (
    <FixtureFrame
      title="Layer Breadcrumb — World Timeline"
      description="World Timeline patterns: Brief only · Brief › Narrative · Narrative only. Parent segment is a zoom-out button; active segment uses aria-current=&quot;page&quot;."
      testId="layer-breadcrumb-fixture-world"
    >
      <VariantMatrix testId="layer-breadcrumb-world-matrix">
        <VariantChip label="Brief only">
          <LayerBreadcrumb
            surfaceKey="fixture-world-brief"
            coarseSegment={{ layer: 'brief', label: 'Brief' }}
            fineSegment={{ layer: 'narrative', label: 'Narrative' }}
            activeLayer="brief"
            onLayerChange={() => {}}
            ariaLabel="Timeline layer path"
          />
        </VariantChip>

        <VariantChip label="Brief › Narrative">
          <LayerBreadcrumb
            surfaceKey="fixture-world-path"
            coarseSegment={{ layer: 'brief', label: 'Brief' }}
            fineSegment={{ layer: 'narrative', label: 'Narrative' }}
            activeLayer="narrative"
            onLayerChange={() => {}}
            ariaLabel="Timeline layer path"
          />
        </VariantChip>

        <VariantChip label="Narrative only (same as path active)">
          <LayerBreadcrumb
            surfaceKey="fixture-world-narrative"
            coarseSegment={{ layer: 'brief', label: 'Brief' }}
            fineSegment={{ layer: 'narrative', label: 'Narrative' }}
            activeLayer="narrative"
            onLayerChange={() => {}}
            ariaLabel="Timeline layer path"
          />
        </VariantChip>

        <VariantChip label="Interactive (click Brief to zoom out)">
          <LayerBreadcrumb
            surfaceKey="fixture-world-live"
            coarseSegment={{ layer: 'brief', label: 'Brief' }}
            fineSegment={{ layer: 'narrative', label: 'Narrative' }}
            activeLayer={active}
            onLayerChange={setActive}
            ariaLabel="Timeline layer path"
          />
        </VariantChip>
      </VariantMatrix>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  Work Timeline — Narrative ↔ Moment                                  */
/* ------------------------------------------------------------------ */

type WorkLayer = 'narrative' | 'moment';

function WorkBreadcrumbMatrix() {
  const [active, setActive] = useState<WorkLayer>('moment');

  return (
    <FixtureFrame
      title="Layer Breadcrumb — Work Timeline"
      description="Work Timeline patterns: Narrative only · Narrative › Moment · Moment only. Same chrome; different layer vocabulary."
      testId="layer-breadcrumb-fixture-work"
    >
      <VariantMatrix testId="layer-breadcrumb-work-matrix">
        <VariantChip label="Narrative only">
          <LayerBreadcrumb
            surfaceKey="fixture-work-narrative"
            coarseSegment={{ layer: 'narrative', label: 'Narrative' }}
            fineSegment={{ layer: 'moment', label: 'Moment' }}
            activeLayer="narrative"
            onLayerChange={() => {}}
            ariaLabel="Work Timeline layer path"
          />
        </VariantChip>

        <VariantChip label="Narrative › Moment">
          <LayerBreadcrumb
            surfaceKey="fixture-work-path"
            coarseSegment={{ layer: 'narrative', label: 'Narrative' }}
            fineSegment={{ layer: 'moment', label: 'Moment' }}
            activeLayer="moment"
            onLayerChange={() => {}}
            ariaLabel="Work Timeline layer path"
          />
        </VariantChip>

        <VariantChip label="Moment only (same as path active)">
          <LayerBreadcrumb
            surfaceKey="fixture-work-moment"
            coarseSegment={{ layer: 'narrative', label: 'Narrative' }}
            fineSegment={{ layer: 'moment', label: 'Moment' }}
            activeLayer="moment"
            onLayerChange={() => {}}
            ariaLabel="Work Timeline layer path"
          />
        </VariantChip>

        <VariantChip label="Interactive (click Narrative to zoom out)">
          <LayerBreadcrumb
            surfaceKey="fixture-work-live"
            coarseSegment={{ layer: 'narrative', label: 'Narrative' }}
            fineSegment={{ layer: 'moment', label: 'Moment' }}
            activeLayer={active}
            onLayerChange={setActive}
            ariaLabel="Work Timeline layer path"
          />
        </VariantChip>
      </VariantMatrix>
    </FixtureFrame>
  );
}

/**
 * Layer breadcrumb fixtures — World + Work patterns, light + dark via theme.
 */
export function LayerBreadcrumbFixtures() {
  return (
    <div data-testid="layer-breadcrumb-fixtures">
      <WorldBreadcrumbMatrix />
      <WorkBreadcrumbMatrix />
    </div>
  );
}
