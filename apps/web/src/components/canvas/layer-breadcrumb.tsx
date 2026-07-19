/**
 * Layer breadcrumb — App re-export of the presentational extract.
 *
 * Implementation lives in `presentational/layer-breadcrumb.tsx` so Design
 * Studio can import via `@web-canvas/layer-breadcrumb` without i18n/daemon.
 * App hosts resolve labels with `t()` before passing props.
 *
 * @see iterations/v1.123/specs/layer-feel-differentiation.md §3.4
 * @see iterations/v1.124/specs/surface-audit-checklist.md §4.2
 */
export {
  LayerBreadcrumb,
  type LayerBreadcrumbProps,
  type LayerBreadcrumbSegment,
} from '@/components/canvas/presentational/layer-breadcrumb';
