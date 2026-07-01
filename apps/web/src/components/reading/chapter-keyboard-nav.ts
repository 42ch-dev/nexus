/**
 * Chapter keyboard navigation — V1.79 Author Reflection (Track A / P0).
 *
 * ←/→ chapter/volume navigation for the reading surface, with guards so the
 * shortcuts never compete with form input, contenteditable, or open overlays
 * (menus/dialogs). Extracted verbatim from {@link ChapterPage} so the
 * keyboard-nav interaction contract is unit-testable in isolation
 * (R-V179P0-QC1-001) and lives alongside the other reading-surface
 * affordances. Behavior is unchanged from the in-page hook it replaced.
 */
import { useEffect } from 'react';
import type { useNavigate } from 'react-router-dom';

type Navigate = ReturnType<typeof useNavigate>;

/**
 * Minimal chapter target the keyboard nav reads from a neighbor row. A full
 * {@link import('@42ch/nexus-contracts').ChapterSummary} satisfies this
 * structurally (it carries `chapter: number` + `volume: number`).
 */
export interface ChapterKeyboardTarget {
  chapter: number;
  volume: number;
}

export interface ChapterKeyboardNeighbors {
  prev: ChapterKeyboardTarget | null;
  next: ChapterKeyboardTarget | null;
}

/**
 * Wire ←/→ keyboard navigation between chapters. The effect is a no-op when
 * the reader is focused in an editable element or a menu/dialog is open, so it
 * never competes with form input or the body context menu.
 *
 * Guards: modifier keys (meta/ctrl/alt) are ignored; a `defaultPrevented` event
 * is ignored; an editable {@link isEditable} or overlay-blocked
 * ({@link hasOpenOverlay}) focus/target short-circuits before navigating.
 */
export function useChapterKeyboardNav(
  workId: string,
  neighbors: ChapterKeyboardNeighbors,
  navigate: Navigate,
): void {
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.defaultPrevented) return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const el = document.activeElement;
      if (el && (isEditable(el) || hasOpenOverlay())) return;
      const target =
        e.key === 'ArrowLeft' ? neighbors.prev : e.key === 'ArrowRight' ? neighbors.next : null;
      if (!target) return;
      e.preventDefault();
      const v = target.volume ?? 1;
      navigate(`/works/${encodeURIComponent(workId)}/chapters/${target.chapter}?volume=${v}`);
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [workId, neighbors, navigate]);
}

/**
 * True when the focused element is a form control or contenteditable region, so
 * the reader can type without arrow keys hijacking chapter navigation. Covers
 * `<input>`, `<textarea>`, `<select>`, and any `contenteditable` element.
 */
export function isEditable(el: Element): boolean {
  const tag = el.tagName;
  return (
    tag === 'INPUT' ||
    tag === 'TEXTAREA' ||
    tag === 'SELECT' ||
    (el as HTMLElement).isContentEditable === true
  );
}

/**
 * A visible menu or dialog captures keyboard intent; do not navigate.
 *
 * Role-pattern assumption (R-V179P0-QC1-001): overlays in this app render with
 * `role="menu"` or `role="dialog"`. When closed they are either unmounted or
 * carry the `[hidden]` attribute, so this query short-circuits keyboard nav
 * only while an overlay is genuinely open. If a future overlay uses a different
 * role (e.g. `role="alertdialog"`) or signals visibility by a class/ARIA state
 * instead of `[hidden]`, extend the selector set here rather than relying on
 * this guard catching it implicitly.
 */
export function hasOpenOverlay(): boolean {
  // A visible menu or dialog captures keyboard intent; do not navigate.
  return Boolean(
    document.querySelector('[role="menu"]:not([hidden]), [role="dialog"]:not([hidden])'),
  );
}
