/**
 * Layer breadcrumb — presentational extract (V1.124 P2).
 *
 * Renders the layer hierarchy path (e.g. `Brief › Narrative` or
 * `Narrative › Moment`) as a clickable zoom-out affordance.
 * Implements the breadcrumb contract from
 * `iterations/v1.123/specs/layer-feel-differentiation.md` §3.4.
 *
 * Presentational boundary:
 *   - No `@xyflow/react`, no daemon, no contracts, no `useTranslation`.
 *   - All labels arrive as resolved strings (App host calls `t()`; Studio
 *     fixtures pass static English product vocabulary).
 *
 * Studio alias: `@web-canvas/layer-breadcrumb`.
 */
export interface LayerBreadcrumbSegment<L extends string> {
  /** Layer discriminator value passed to `onLayerChange`. */
  layer: L;
  /** Resolved segment label (already translated by the host). */
  label: string;
}

export interface LayerBreadcrumbProps<L extends string> {
  /**
   * Stable surface discriminator used in test ids — e.g. `timeline` or
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
  /** Resolved aria-label for the breadcrumb nav. */
  ariaLabel: string;
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
  ariaLabel,
}: LayerBreadcrumbProps<L>) {
  const isCoarseActive = activeLayer === coarseSegment.layer;
  const isFineActive = activeLayer === fineSegment.layer;

  // The breadcrumb always renders the active segment; the parent segment
  // renders only when the user has drilled into the fine layer. When the
  // coarse layer is active, the breadcrumb shows just the coarse label.
  return (
    <nav
      data-testid={`${surfaceKey}-layer-breadcrumb`}
      aria-label={ariaLabel}
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
            {coarseSegment.label}
          </button>
          <span aria-hidden="true" className="text-gray-500">
            ›
          </span>
          <span
            data-testid={`${surfaceKey}-layer-breadcrumb-segment-${fineSegment.layer}`}
            aria-current="page"
            className="rounded-control px-1.5 py-0.5 font-semibold text-gray-1000"
          >
            {fineSegment.label}
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
          {isCoarseActive ? coarseSegment.label : fineSegment.label}
        </span>
      )}
    </nav>
  );
}
