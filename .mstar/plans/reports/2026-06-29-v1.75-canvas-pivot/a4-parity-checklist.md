# V1.75 A4 — Canvas-Pivot Parity Checklist

**plan_id**: `2026-06-29-v1.75-canvas-pivot`
**Task**: A4 — parity verification (gate before A5 retire)
**Result**: ✅ PASS — canvas inspector matches V1.65 editor capability

The V1.65 whole-document TipTap outline editor (`apps/web/src/pages/chapter-page.tsx`)
and the V1.75 in-inspector editor
(`apps/web/src/components/canvas/outline-canvas/inspectors/chapter-outline-content-editor.tsx`)
use the **same TipTap extensions**: `StarterKit` + `tiptap-markdown`'s `Markdown`.
Parity is therefore structural — both editors serialize markdown identically.

## Rich-text capability checklist

| Capability | V1.65 chapter-page | V1.75 canvas inspector | Parity |
|------------|--------------------|------------------------|--------|
| Heading 1 / 2 / 3 | `toggleHeading({level})` | `toggleHeading({level})` | ✅ |
| Bold | `toggleBold` | `toggleBold` | ✅ |
| Italic | `toggleItalic` | `toggleItalic` | ✅ |
| Bullet list | `toggleBulletList` | `toggleBulletList` | ✅ |
| Ordered list | `toggleOrderedList` | `toggleOrderedList` | ✅ |
| Blockquote | `toggleBlockquote` | `toggleBlockquote` | ✅ |
| Markdown round-trip | `tiptap-markdown` | `tiptap-markdown` | ✅ |
| Toolbar `aria-pressed` on toggles | yes | yes | ✅ |
| Read-only on protected chapter | `can_edit_outline` gate | `published` gate | ✅ |

## Automated proof

`chapter-outline-content-editor.test.ts` exercises the **real** TipTap `Editor`
(not a mock) with `StarterKit + Markdown` and proves:

- each block type (h1/h2/h3, bold, italic, bullet list, ordered list,
  blockquote) survives `markdown → editor → getMarkdown()` with its text
  content intact;
- a mixed realistic outline document round-trips preserving every block;
- two successive round-trips converge byte-for-byte (no drift / no phantom
  dirty state from re-escaping).

This locks the serialization contract. A `tiptap-markdown` or `StarterKit`
upgrade that breaks the round-trip fails this test **before** the V1.65 retire
(A5) lands, keeping the pivot parity-safe.

## Read path parity

| Surface | V1.65 | V1.75 |
|---------|-------|-------|
| Content source | `GET /chapters/{n}/outline` → `outline_path` | same `GET /chapters/{n}/outline` (kept) → `outline_path` |
| Write path | `PUT /chapters/{n}/outline` (`PutChapterOutlineRequest`) | `POST /chapters/{n}/patch` `set.content` (outline_revision CAS + outline_path) |

The V1.75 write rides the work-level `outline_revision` CAS (conflict modal on
409) — a strictness improvement over V1.65's lockless PUT. The per-chapter
`outline_path` markdown file is the shared persistence target, so content
authored in either editor is readable by the other (during the cutover) and by
the daemon's existing readers.

## Gate

A4 = **PASS**. Phase 2 (A5 retire + A6 dead-code) may proceed.
