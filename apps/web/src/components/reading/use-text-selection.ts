/**
 * useTextSelection — V1.89 Deeper Manuscript Reading.
 *
 * Captures text selections that fall inside a given container and converts them
 * to character offsets within the container's rendered text content. The
 * returned selection is used to create persistent highlights anchored to the
 * prose body.
 */
import { useEffect, useRef, useState, type RefObject } from 'react';

export interface TextSelection {
  /** The selected text. */
  text: string;
  /** Inclusive character offset where the selection starts. */
  startOffset: number;
  /** Exclusive character offset where the selection ends. */
  endOffset: number;
}

export interface TextSelectionState {
  selection: TextSelection | null;
  /** Viewport-relative position suitable for positioning a floating toolbar. */
  position: { x: number; y: number } | null;
  /** Clear the captured selection (e.g. after creating a highlight). */
  clear: () => void;
}

/**
 * Compute the plain-text offset of a DOM point inside a container by walking
 * all text nodes in document order.
 */
function pointToOffset(container: HTMLElement, node: Node, nodeOffset: number): number {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  let offset = 0;
  let currentNode: Node | null;
  while ((currentNode = walker.nextNode())) {
    const text = currentNode.textContent ?? '';
    if (currentNode === node) {
      return offset + Math.min(nodeOffset, text.length);
    }
    offset += text.length;
  }
  return offset;
}

/**
 * Extract the selected text and start/end offsets relative to the container's
 * full text content.
 */
function getSelectionOffsets(container: HTMLElement, range: Range): TextSelection {
  const startOffset = pointToOffset(container, range.startContainer, range.startOffset);
  const endOffset = pointToOffset(container, range.endContainer, range.endOffset);
  const fullText = container.textContent ?? '';
  return {
    text: fullText.slice(startOffset, endOffset),
    startOffset,
    endOffset,
  };
}

export function useTextSelection(containerRef: RefObject<HTMLElement | null>): TextSelectionState {
  const [selection, setSelection] = useState<TextSelection | null>(null);
  const [position, setPosition] = useState<{ x: number; y: number } | null>(null);
  const pendingRange = useRef<Range | null>(null);

  const clear = () => {
    setSelection(null);
    setPosition(null);
    pendingRange.current = null;
    const sel = window.getSelection();
    if (sel) sel.removeAllRanges();
  };

  useEffect(() => {
    function handleSelectionChange() {
      const container = containerRef.current;
      if (!container) return;

      const sel = window.getSelection();
      if (!sel || sel.isCollapsed) {
        setSelection(null);
        setPosition(null);
        pendingRange.current = null;
        return;
      }

      const range = sel.getRangeAt(0);
      if (!container.contains(range.commonAncestorContainer)) {
        setSelection(null);
        setPosition(null);
        pendingRange.current = null;
        return;
      }

      pendingRange.current = range;
      const offsets = getSelectionOffsets(container, range);
      if (offsets.endOffset <= offsets.startOffset) {
        setSelection(null);
        setPosition(null);
        return;
      }

      const rect = range.getBoundingClientRect();
      setSelection(offsets);
      setPosition({ x: rect.left + rect.width / 2, y: rect.top });
    }

    document.addEventListener('selectionchange', handleSelectionChange);
    return () => document.removeEventListener('selectionchange', handleSelectionChange);
  }, [containerRef]);

  return { selection, position, clear };
}

/**
 * Create a DOM Range from character offsets inside a container's text nodes.
 * Returns null if the offsets do not map to a valid range.
 */
export function rangeFromOffsets(
  container: HTMLElement,
  startOffset: number,
  endOffset: number,
): Range | null {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  let offset = 0;
  let startNode: Node | null = null;
  let startNodeOffset = 0;
  let endNode: Node | null = null;
  let endNodeOffset = 0;
  let node: Node | null;

  while ((node = walker.nextNode())) {
    const text = node.textContent ?? '';
    const nodeStart = offset;
    const nodeEnd = offset + text.length;

    if (startNode === null && startOffset < nodeEnd) {
      startNode = node;
      startNodeOffset = Math.max(0, startOffset - nodeStart);
    }
    if (endNode === null && endOffset <= nodeEnd) {
      endNode = node;
      endNodeOffset = Math.max(0, endOffset - nodeStart);
      break;
    }
    offset += text.length;
  }

  if (!startNode || endNode === null) return null;
  const range = document.createRange();
  try {
    range.setStart(startNode, startNodeOffset);
    range.setEnd(endNode, endNodeOffset);
  } catch {
    return null;
  }
  return range;
}
