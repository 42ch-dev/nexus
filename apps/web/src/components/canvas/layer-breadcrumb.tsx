/**
 * Layer breadcrumb — V1.123 P4 Task 5.
 *
 * Renders the layer hierarchy path (e.g., `Brief > Narrative` or
 * `Narrative > Moment`) as a clickable zoom-out affordance.
 * Implements the breadcrumb contract from
 * `iterations/v1.123/specs/layer-feel-differentiation.md` §3.4:
 *
 *   | Surface        | Breadcrumb pattern                            |
 *   |----------------|------------------------------------------------|
 *   | World Timeline | `Brief` or `Brief > Narrative` or `Narrative`  |
 *   | Work Timeline  | `Narrative` or `Narrative > Moment` or `Moment`|
 *
 * Breadcrumbs are clickable zoom-out targets (parent layer): the parent
 * segment (`coarse`) is a button that calls `onLayerChange(coarse)`; the
 * active child segment (`fine`) renders as static text with `aria-current`.
 *
 * When the active layer is the topmost (coarse) layer, only the coarse
 * segment renders (no parent to zoom out to) — the breadcrumb still surfaces
 * the layer's place in the hierarchy as a non-interactive label.
 *
 * Shared by both Timeline (Brief ↔ Narrative) and Work Timeline
 * (Narrative ↔ Moment) canvases — each surface supplies its own `surfaceKey`
 * (for test id stability) + labels. Built as a small inline-style component
 * (not promoted to `@42ch/nexus-ui`) because the layer-chain shape is
 * canvas-specific (not a generic primitive); if a third surface arrives with
 * the same pattern, promotion becomes worth the abstraction cost.
 *
 * Accessibility:
 *   - The breadcrumb `<nav>` carries an `aria-label` so SR users can find it.
 *   - The parent segment is a real `<button>` (focusable, keyboard-activeable).
 *   - The active segment carries `aria-current="page"` so SR users hear the
 *     current layer as "current page" semantics (WCAG 2.1 — breadcrumb
 *     current-page indicator).
 */
import { useTranslation } from 'react-i18next';

export interface LayerBreadcrumbSegment<L extends string> {
  /** Layer discriminator value passed to `onLayerChange`. */
  layer: L;
  /** i18n key (under the `canvas` namespace) for the segment label. */
  labelKey: string;
  /** Fallback label if the i18n key is missing (mirrors existing switchers). */
  defaultValue: string;
}

export interface LayerBreadcrumbProps<L extends string> {
  /**
   * Stable surface discriminator used in test ids — e.g., `timeline` or
   * `work-timeline`. Test ids are `${surfaceKey}-layer-breadcrumb` and
   * `${surfaceKey}-layer-breadcrumb-segment-${layer}`.
   */
  surfaceKey: string;
  /** Parent (coarse-zoom) layer — zoom-out target. */
  coarseSegment: LayerBreadcrumbSegment<L>;
  /** Child (fine-zoom) layer — entered when the user drills in. */
  fineSegment: LayerBreadcrumbSegment<L>;
  /** Currently active layer — drives which segment is interactive. */
  activeLayer: L;
  /** Layer swap callback — same callback the layer switcher tabs use. */
  onLayerChange: (layer: L) => void;
  /** i18n aria-label key for the breadcrumb nav. */
  ariaLabelKey: string;
  /** Fallback aria-label if the i18n key is missing. */
  ariaLabelDefaultValue: string;
}

/**
 * Render the layer breadcrumb. See file docstring for the contract.
 */
export function LayerBreadcrumb<L extends string>({
  surfaceKey,
  coarseSegment,
  fineSegment,
  activeLayer,
  onLayerChange,
  ariaLabelKey,
  ariaLabelDefaultValue,
}: LayerBreadcrumbProps<L>) {
  const { t } = useTranslation('canvas');
  const isCoarseActive = activeLayer === coarseSegment.layer;
  const isFineActive = activeLayer === fineSegment.layer;

  // The breadcrumb always renders the active segment; the parent segment
  // renders only when the user has drilled into the fine layer (i.e., the
  // fine layer is active). When the coarse layer is active, the breadcrumb
  // shows just the coarse label (no parent to zoom out to).
  return (
    <nav
      data-testid={`${surfaceKey}-layer-breadcrumb`}
      aria-label={t(ariaLabelKey, { defaultValue: ariaLabelDefaultValue })}
      className="flex items-center gap-1 text-copy-12 text-gray-700"
    >
      {isFineActive ? (
        <>
          <button
            type="button"
            data-testid={`${surfaceKey}-layer-breadcrumb-segment-${coarseSegment.layer}`}
            onClick={() => onLayerChange(coarseSegment.layer)}
            className="rounded-control px-1.5 py-0.5 text-gray-700 hover:bg-gray-alpha-100 hover:text-gray-1000 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
          >
            {t(coarseSegment.labelKey, { defaultValue: coarseSegment.defaultValue })}
          </button>
          <span aria-hidden="true" className="text-gray-500">
            ›
          </span>
          <span
            data-testid={`${surfaceKey}-layer-breadcrumb-segment-${fineSegment.layer}`}
            aria-current="page"
            className="rounded-control px-1.5 py-0.5 font-semibold text-gray-1000"
          >
            {t(fineSegment.labelKey, { defaultValue: fineSegment.defaultValue })}
          </span>
        </>
      ) : (
        <span
          data-testid={`${surfaceKey}-layer-breadcrumb-segment-${
            isCoarseActive ? coarseSegment.layer : fineSegment.layer
          }`}
          aria-current="page"
          className="rounded-control px-1.5 py-0.5 font-semibold text-gray-1000"
        >
          {t(
            isCoarseActive ? coarseSegment.labelKey : fineSegment.labelKey,
            {
              defaultValue: isCoarseActive
                ? coarseSegment.defaultValue
                : fineSegment.defaultValue,
            },
          )}
        </span>
      )}
    </nav>
  );
}
