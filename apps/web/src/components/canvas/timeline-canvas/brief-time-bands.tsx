/**
 * Brief time-band renderer — V1.159 P1 Task 2.
 *
 * Renders the `EraTreeNode[]` forest (Task 1 — `buildEraTree`) as vertical
 * time bands (geological-strata metaphor) per
 * `.mstar/specs/canvas-strategy-surface.md` §3.3.3 (V1.159 amendment):
 *   - Each era = one horizontal band; nested eras = indented sub-bands
 *     (`padding-left` = `depth × INDENT_UNIT_PX`).
 *   - Per-`era_type` band coloring. The spec's `--color-era-type-*` DESIGN.md
 *     token family is pending architect sign-off, so this renderer maps the
 *     recommended types onto existing DESIGN.md tokens today (a hue ramp off
 *     the `--color-canvas-layer-brief-accent` spine — kingdom/age/epoch are
 *     the amber steps, period is the Timeline brand-blue accent, sub-age is
 *     muted gray); unknown/absent `era_type` falls back to the default Brief
 *     accent (legacy V1.123 flat-era data renders compatibly). When the
 *     DESIGN.md family lands, swap the map values for `var(--color-era-type-*)`
 *     — the component reads colors through semantic CSS custom properties.
 *   - Band content: `canonical_name` (primary) + optional `world_summary`
 *     snippet from `body.attributes.world_summary` (secondary, clamped) +
 *     type badge (verbatim freeform `era_type` when present).
 *   - Expand/collapse: bands at `depth > 0` with children are collapsible
 *     via a caret toggle (all bands default to expanded); depth-0 bands are
 *     always expanded (spec + Task 2 DoD).
 *   - Read-only rendering: the band content button fires `onSelectEra` only
 *     (no inline edit; era creation is Task 3).
 *
 * Indentation is absolute per node (`padding-left = depth × indent unit`) so
 * children render as siblings in the DOM — no inherited-padding math.
 */
import { Fragment, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronDown, ChevronRight } from 'lucide-react';

import type { WorldKbEntityProjection } from '@42ch/nexus-contracts';

import type { EraTreeNode } from './brief-era-tree';

export interface BriefTimeBandsProps {
  /** The era forest from `buildEraTree()` (Task 1). */
  tree: EraTreeNode[];
  /**
   * Selection hand-off. Fires with the era's `key_block_id` when a band's
   * content is activated; the adapter routes it through the existing
   * React Flow selection flow so the era inspector opens (selection-only —
   * the time bands perform no writes).
   */
  onSelectEra?: (eraId: string) => void;
}

/**
 * Indentation unit per nesting level (DESIGN.md `spacing.space-6` = 24px).
 * Nested bands shift right by one indent step per `depth` (spec §3.3.3).
 */
const INDENT_UNIT_PX = 24;

/**
 * Per-`era_type` band colors — existing DESIGN.md tokens (both themes).
 * `simplify:` the spec's `--color-era-type-*` token family is pending
 * architect sign-off (spec §3.3.3 type-coloring row); this map uses shipped
 * tokens as the interim ramp. Swap values for the DESIGN.md family verbatim
 * once it lands — no component shape change needed.
 */
const ERA_TYPE_COLORS: Readonly<Record<string, string>> = {
  // Coarsest stratum → strongest bronze (kingdom), stepping lighter (age,
  // epoch) off the `--color-canvas-layer-brief-accent` spine.
  kingdom: 'var(--color-amber-900)',
  age: 'var(--color-amber-800)',
  epoch: 'var(--color-amber-700)',
  // Accent tier — Timeline brand-blue (canvas-timeline-accent spine).
  period: 'var(--color-blue-1000)',
  // Finest stratum → muted.
  'sub-age': 'var(--color-gray-700)',
};

/** Unknown/absent `era_type` → default Brief accent (legacy-compatible). */
const DEFAULT_ERA_COLOR = 'var(--color-canvas-layer-brief-accent)';

/** Band text rides on the colored fill — theme-independent white. */
const BAND_TEXT_COLOR = 'var(--color-brand-white)';

function eraColorOf(eraType: string | undefined): string {
  if (eraType === undefined) return DEFAULT_ERA_COLOR;
  return ERA_TYPE_COLORS[eraType] ?? DEFAULT_ERA_COLOR;
}

/**
 * Extract the optional world-summary snippet from `body.attributes`.
 * Type narrowing mirrors `eraTypeOf` in `brief-era-tree.ts` — attributes are
 * `unknown` until narrowed; non-string / empty values are absent.
 */
function worldSummaryOf(entity: WorldKbEntityProjection): string | undefined {
  const attrs = entity.body?.attributes;
  if (attrs === null || typeof attrs !== 'object') return undefined;
  const raw = (attrs as Record<string, unknown>).world_summary;
  return typeof raw === 'string' && raw.length > 0 ? raw : undefined;
}

export function BriefTimeBands({ tree, onSelectEra }: BriefTimeBandsProps) {
  const { t } = useTranslation('canvas');
  const [collapsedIds, setCollapsedIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );

  // Empty forest → no bands (hooks above already ran). The no-era-data copy
  // is owned by the V1.123 Brief empty-state panel in `timeline-canvas.tsx`
  // (`isBriefEmpty` gates it upstream, so this branch is defense-in-depth
  // for direct callers): the renderer must not fabricate a band surface for
  // a zero-node tree.
  if (tree.length === 0) return null;

  const toggle = (eraId: string): void => {
    setCollapsedIds((prev) => {
      const next = new Set(prev);
      if (next.has(eraId)) {
        next.delete(eraId);
      } else {
        next.add(eraId);
      }
      return next;
    });
  };

  const renderBand = (node: EraTreeNode): ReactNode => {
    const { era, era_type, children, depth } = node;
    const eraId = era.key_block_id;
    const title = era.canonical_name || t('timeline.briefEraNode.unnamed');
    const worldSummary = worldSummaryOf(era);
    // DoD: bands at depth > 0 are collapsible; depth-0 bands stay expanded.
    const isCollapsible = depth > 0 && children.length > 0;
    const isCollapsed = collapsedIds.has(eraId);
    const toggleLabel = isCollapsed
      ? t('timeline.briefTimeBands.expand', { name: title })
      : t('timeline.briefTimeBands.collapse', { name: title });

    return (
      <Fragment key={eraId}>
        <div
          style={{ paddingLeft: depth * INDENT_UNIT_PX }}
          data-depth={depth}
          data-era-id={eraId}
        >
          <div
            className="flex flex-col gap-1 rounded-control border border-gray-alpha-400 px-3 py-2 shadow-elevation-1"
            style={{
              backgroundColor: eraColorOf(era_type),
              color: BAND_TEXT_COLOR,
            }}
            data-testid="brief-time-band"
            data-era-id={eraId}
            data-era-type={era_type ?? ''}
          >
            <div className="flex items-center gap-2">
              {isCollapsible ? (
                <button
                  type="button"
                  onClick={() => toggle(eraId)}
                  aria-expanded={!isCollapsed}
                  aria-label={toggleLabel}
                  className="flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-control transition-colors hover:bg-white/20"
                  data-testid="brief-time-band-toggle"
                >
                  {isCollapsed ? (
                    <ChevronRight className="h-4 w-4" aria-hidden />
                  ) : (
                    <ChevronDown className="h-4 w-4" aria-hidden />
                  )}
                </button>
              ) : null}
              <button
                type="button"
                onClick={() => onSelectEra?.(eraId)}
                className="flex min-w-0 flex-1 items-center gap-2 text-left"
                data-testid="brief-time-band-content"
              >
                <span
                  className="truncate font-heading text-copy-14 font-semibold"
                  title={title}
                >
                  {title}
                </span>
                {era_type ? (
                  <span
                    className="rounded-pill border border-white/30 bg-white/15 px-1.5 py-0.5 text-label-12"
                    data-testid="brief-time-band-type-badge"
                  >
                    {era_type}
                  </span>
                ) : null}
              </button>
            </div>
            {worldSummary ? (
              <p
                className="line-clamp-2 text-label-12"
                title={worldSummary}
                data-testid="brief-time-band-summary"
              >
                {worldSummary}
              </p>
            ) : null}
          </div>
        </div>
        {!isCollapsed ? children.map(renderBand) : null}
      </Fragment>
    );
  };

  return (
    <div
      className="flex flex-col gap-2"
      role="group"
      aria-label={t('timeline.briefTimeBands.ariaLabel')}
      data-testid="brief-time-bands"
    >
      {tree.map(renderBand)}
    </div>
  );
}
