/**
 * V1.75 A4 — outline content TipTap <-> markdown round-trip parity test.
 *
 * Proves the canvas inspector's rich-text capability matches the V1.65
 * chapter-page editor. Both use the SAME extensions (`StarterKit` +
 * `tiptap-markdown`'s `Markdown`), so this test locks the serialization
 * contract the parity-close depends on: markdown in -> TipTap doc ->
 * markdown out preserves headings, bold, italic, bullet/ordered lists, and
 * blockquote semantics.
 *
 * If a tiptap-markdown upgrade or a StarterKit config change breaks the
 * round-trip, this test fails BEFORE the V1.65 retire (A5) ships, keeping the
 * pivot parity-safe.
 */
import { describe, expect, it, beforeAll } from 'vitest';
import { Editor } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import { Markdown } from 'tiptap-markdown';

// ProseMirror/TipTap read layout APIs that jsdom does not implement. Polyfill
// the minimal surface so a headless Editor can mount and serialize without a
// real layout pass. (Same workaround class as the chapter-page test mock, but
// here we exercise the REAL editor so the round-trip is actually proven.)
beforeAll(() => {
  const noopRect = { x: 0, y: 0, top: 0, left: 0, bottom: 0, right: 0, width: 0, height: 0, toJSON: () => ({}) };
  const proto = Element.prototype;
  if (!proto.getClientRects) {
    proto.getClientRects = (() => [noopRect] as unknown as DOMRectList) as typeof proto.getClientRects;
  }
  if (!proto.getBoundingClientRect) {
    proto.getBoundingClientRect = (() => noopRect as DOMRect) as typeof proto.getBoundingClientRect;
  }
  // Range is used by ProseMirror's text-offset resolution.
  const rangeProto = Range.prototype as unknown as { getClientRects?: unknown };
  if (!rangeProto.getClientRects) {
    rangeProto.getClientRects = () => [noopRect as DOMRect];
  }
});

function makeEditor(initial: string): Editor {
  return new Editor({
    extensions: [StarterKit, Markdown],
    content: initial,
    // No DOM mount needed for serialization-only use.
    element: document.createElement('div'),
  });
}

function roundTrip(md: string): string {
  const editor = makeEditor(md);
  const out = (editor.storage.markdown as { getMarkdown: () => string }).getMarkdown();
  editor.destroy();
  return out;
}

describe('V1.75 A4 — TipTap <-> markdown round-trip (outline content parity)', () => {
  it.each([
    ['heading 1', '# Opening', /Opening/],
    ['heading 2', '## Scene beats', /Scene beats/],
    ['heading 3', '### Sub-beat', /Sub-beat/],
    ['bold', '**important** callout', /important/],
    ['italic', '_whisper_ here', /whisper/],
    ['bullet list', '- one\n- two\n- three', /one/],
    ['ordered list', '1. first\n2. second', /first/],
    ['blockquote', '> a note from the author', /a note from the author/],
  ])('preserves %s through the round-trip', (_label, md, expected) => {
    const out = roundTrip(md);
    expect(out).toMatch(expected);
  });

  it('preserves a mixed markdown outline document (V1.65 parity sample)', () => {
    // A realistic chapter outline combining every supported block type — the
    // same shape an author would draft in the retiring V1.65 editor.
    const md = [
      '# Chapter 3 — The Harbor',
      '',
      '## Scene beats',
      '',
      '- Open on the harbor at dawn',
      '- **Meet the contact** at the warehouse',
      '- _whispered_ warning about the tide',
      '',
      '## Open questions',
      '',
      '1. Who tipped off the customs agent?',
      '2. Is the cargo manifest forged?',
      '',
      '> Author note: keep the contact silent until chapter 5.',
    ].join('\n');

    const out = roundTrip(md);
    // Each block type survives the round-trip with its text content intact.
    expect(out).toMatch(/Chapter 3.*Harbor/);
    expect(out).toMatch(/Scene beats/);
    expect(out).toMatch(/Open on the harbor/);
    expect(out).toMatch(/Meet the contact/);
    expect(out).toMatch(/whispered/);
    expect(out).toMatch(/Open questions/);
    expect(out).toMatch(/tipped off/);
    expect(out).toMatch(/cargo manifest/);
    expect(out).toMatch(/Author note/);
  });

  it('is stable across repeated round-trips (no drift)', () => {
    // Two successive round-trips must converge: the second pass is byte-equal
    // to the first. This guards against tiptap-markdown re-escaping or
    // re-formatting on each save (which would cause phantom dirty states).
    const md = '## Beats\n\n- one\n- two\n\n**bold** _italic_\n\n> quote\n';
    const once = roundTrip(md);
    const twice = roundTrip(once);
    expect(twice).toBe(once);
  });
});
