/**
 * Outline canvas — chapter outline content editor (V1.75 A3 canvas-pivot).
 *
 * TipTap rich-text editor for the chapter's outline prose notes, mounted inside
 * the chapter inspector below the metadata fields. Closes the V1.65 parity gap:
 * the editor replicates the retired chapter-page TipTap capability (headings,
 * bold, italic, lists, blockquote, markdown round-trip via `tiptap-markdown`).
 *
 * Read path: `useChapterOutline` (V1.65 GET /chapters/{n}/outline, kept as the
 * inspector content read in A6). Write path: `onPatchChapter` with
 * `set.content`, which rides the V1.72 `outline_revision` CAS + the per-chapter
 * `outline_path` persistence landed in A2. On 409 the canvas orchestrator
 * surfaces the shared outline conflict modal (with the `chapter_outline_content`
 * field label added in A3).
 *
 * Extracted as a sibling module so `chapter-inspector.tsx` stays ≤250 lines
 * (V1.73 split cap). The toolbar + editor mirror the V1.65 chapter-page editor
 * behavior so authors do not lose rich-text capability in the pivot.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Bold,
  Heading1,
  Heading2,
  Heading3,
  Italic,
  List,
  ListOrdered,
  Loader2,
  Quote,
  RotateCcw,
} from 'lucide-react';
import { EditorContent, useEditor, type Editor } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import { Markdown } from 'tiptap-markdown';

import { Button } from '@/components/ui/button';
import { useChapterOutline } from '@/api/queries';
import type { OutlinePatchChapterRequest } from '@42ch/nexus-contracts';

type SaveState = 'clean' | 'dirty' | 'saving';

interface ChapterOutlineContentEditorProps {
  workId: string;
  chapterNumber: number;
  baseRevision: number;
  /** Volume query for the outline read; defaults to volume 1 server-side. */
  volume?: number;
  disabled: boolean;
  /**
   * Patch dispatcher shared with the metadata fields. Saves emit a content-only
   * `set` so the conflict modal can label the edit `chapter_outline_content`.
   */
  onPatchChapter: (chapter: number, request: OutlinePatchChapterRequest) => void;
  /**
   * Patch mutation status from the canvas orchestrator, used to clear the local
   * dirty flag once a content save commits. The orchestrator owns the canonical
   * pending/conflict state.
   */
  patchIsPending: boolean;
  /** Bumped whenever the orchestrator wants the editor to reset (e.g. after a
   * successful refetch following a conflict resolution). */
  contentVersion: number;
}

export function ChapterOutlineContentEditor({
  workId,
  chapterNumber,
  baseRevision,
  volume,
  disabled,
  onPatchChapter,
  patchIsPending,
  contentVersion,
}: ChapterOutlineContentEditorProps) {
  const volumeQuery = useMemo(
    () => (volume !== undefined && volume > 0 ? { volume } : undefined),
    [volume],
  );
  const outline = useChapterOutline(workId, chapterNumber, volumeQuery);

  const [saveState, setSaveState] = useState<SaveState>('clean');

  // Pin editor deps so useEditor does not re-initialize on every render.
  const editorExtensions = useMemo(() => [StarterKit, Markdown], []);

  const handleEditorUpdate = useCallback(() => {
    setSaveState((prev) => (prev === 'saving' ? prev : 'dirty'));
  }, []);

  const editor = useEditor({
    extensions: editorExtensions,
    content: outline.data?.content ?? '',
    editable: !disabled,
    onUpdate: handleEditorUpdate,
  });

  // Sync editable when the disabled flag flips after mount.
  useEffect(() => {
    if (editor && editor.isEditable === disabled) {
      editor.setEditable(!disabled);
    }
  }, [editor, disabled]);

  // Reset editor content when the outline read resolves or the canvas signals a
  // content reset (post-conflict refetch). Never clobber an in-progress edit:
  // contentVersion bumps after EVERY chapter patch (metadata-only saves
  // included), and the outline read is not re-fetched on those, so without the
  // dirty/saving guard a title or status save would overwrite the editor with a
  // stale server snapshot and silently discard the user's edits.
  useEffect(() => {
    if (!editor || !outline.data || outline.isFetching) return;
    if (saveState === 'dirty' || saveState === 'saving') return;
    const current = getMarkdown(editor);
    if (current !== outline.data.content) {
      editor.commands.setContent(outline.data.content, false);
      setSaveState('clean');
    }
    // contentVersion is an intentional reset trigger.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor, outline.data, outline.isFetching, contentVersion, saveState]);

  // Clear the local dirty flag when the orchestrator's patch mutation settles.
  useEffect(() => {
    if (!patchIsPending && saveState === 'saving') {
      // If we transitioned out of saving without an error callback, treat as
      // clean. Error UX is owned by the canvas conflict modal (409) and the
      // shared error toast.
      setSaveState('clean');
    }
  }, [patchIsPending, saveState]);

  function handleSave() {
    if (!editor) return;
    const content = getMarkdown(editor);
    setSaveState('saving');
    onPatchChapter(chapterNumber, {
      work_id: workId,
      chapter_id: chapterNumber,
      base_revision: baseRevision,
      set: { content },
    });
  }

  function handleReset() {
    if (!editor || !outline.data) return;
    editor.commands.setContent(outline.data.content, false);
    setSaveState('clean');
  }

  if (outline.isLoading) {
    return (
      <div className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 text-copy-14 text-gray-700">
        Loading outline content…
      </div>
    );
  }

  if (outline.isError) {
    return (
      <div className="rounded-card border border-amber-700/30 bg-[color-mix(in_srgb,var(--color-amber-700)_8%,transparent)] p-4 text-copy-14 text-amber-1000">
        Could not load this chapter&rsquo;s outline content. Saving is disabled until the read succeeds.
      </div>
    );
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-copy-13 font-medium text-gray-700">Outline content</span>
        <SaveStateIndicator state={saveState} />
      </div>
      <div className="overflow-hidden rounded-card border border-gray-alpha-400 bg-background-100 focus-within:border-blue-700 focus-within:ring-2 focus-within:ring-blue-700/20">
        <EditorToolbar editor={editor} disabled={disabled} />
        <div
          className="min-h-[240px] p-4"
          role="region"
          aria-label={`Chapter ${chapterNumber} outline content editor`}
        >
          <EditorContent
            editor={editor}
            className="prose prose-sm max-w-none text-copy-16 text-gray-1000"
          />
        </div>
        <div className="flex items-center justify-end gap-2 border-t border-gray-alpha-400 bg-background-200 px-3 py-2">
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={handleReset}
            disabled={disabled || saveState === 'clean' || saveState === 'saving'}
          >
            <RotateCcw className="h-4 w-4" aria-hidden /> Reset
          </Button>
          <Button
            type="button"
            variant="primary"
            size="small"
            onClick={handleSave}
            disabled={disabled || saveState === 'saving' || saveState !== 'dirty'}
          >
            {saveState === 'saving' && <Loader2 className="h-4 w-4 animate-spin" aria-hidden />}
            Save content
          </Button>
        </div>
      </div>
    </div>
  );
}

function getMarkdown(editor: Editor): string {
  return (editor.storage.markdown as { getMarkdown: () => string }).getMarkdown();
}

function SaveStateIndicator({ state }: { state: SaveState }) {
  const config: Record<SaveState, { dot: string; label: string }> = {
    clean: { dot: 'bg-green-700', label: 'Saved' },
    dirty: { dot: 'bg-amber-700', label: 'Unsaved changes' },
    saving: { dot: 'bg-amber-700', label: 'Saving…' },
  };
  const { dot, label } = config[state];
  return (
    <div
      className="flex items-center gap-1.5 text-label-12 text-gray-900"
      aria-live="polite"
      aria-label="Outline content save state"
    >
      <span className={`h-2 w-2 rounded-pill ${dot}`} aria-hidden />
      <span className="max-w-[180px] truncate">{label}</span>
    </div>
  );
}

function EditorToolbar({ editor, disabled }: { editor: Editor | null; disabled: boolean }) {
  if (!editor) return null;
  const toggle = (name: string) => {
    if (disabled) return;
    const chain = editor.chain().focus();
    if (name === 'bold') chain.toggleBold().run();
    else if (name === 'italic') chain.toggleItalic().run();
    else if (name === 'bulletList') chain.toggleBulletList().run();
    else if (name === 'orderedList') chain.toggleOrderedList().run();
    else if (name === 'blockquote') chain.toggleBlockquote().run();
    else if (name.startsWith('heading')) {
      const level = Number(name.replace('heading', '')) as 1 | 2 | 3;
      chain.toggleHeading({ level }).run();
    }
  };
  const ToolbarButton = ({
    name,
    active,
    children,
    title,
  }: {
    name: string;
    active: boolean;
    children: React.ReactNode;
    title: string;
  }) => (
    <button
      type="button"
      onClick={() => toggle(name)}
      title={title}
      aria-label={title}
      aria-pressed={active}
      disabled={disabled}
      className={`flex h-8 w-8 items-center justify-center rounded-control text-gray-1000 transition-colors duration-state ease-standard disabled:cursor-not-allowed disabled:opacity-50 ${
        active ? 'bg-gray-alpha-200' : 'hover:bg-gray-alpha-100'
      }`}
    >
      {children}
    </button>
  );
  return (
    <div
      className="flex flex-wrap items-center gap-1 border-b border-gray-alpha-400 bg-background-200 px-2 py-1"
      role="toolbar"
      aria-label="Outline content formatting"
    >
      <ToolbarButton name="heading1" active={editor.isActive('heading', { level: 1 })} title="Heading 1">
        <Heading1 className="h-4 w-4" aria-hidden />
      </ToolbarButton>
      <ToolbarButton name="heading2" active={editor.isActive('heading', { level: 2 })} title="Heading 2">
        <Heading2 className="h-4 w-4" aria-hidden />
      </ToolbarButton>
      <ToolbarButton name="heading3" active={editor.isActive('heading', { level: 3 })} title="Heading 3">
        <Heading3 className="h-4 w-4" aria-hidden />
      </ToolbarButton>
      <span className="mx-1 h-5 w-px bg-gray-alpha-400" aria-hidden />
      <ToolbarButton name="bold" active={editor.isActive('bold')} title="Bold">
        <Bold className="h-4 w-4" aria-hidden />
      </ToolbarButton>
      <ToolbarButton name="italic" active={editor.isActive('italic')} title="Italic">
        <Italic className="h-4 w-4" aria-hidden />
      </ToolbarButton>
      <span className="mx-1 h-5 w-px bg-gray-alpha-400" aria-hidden />
      <ToolbarButton name="bulletList" active={editor.isActive('bulletList')} title="Bullet list">
        <List className="h-4 w-4" aria-hidden />
      </ToolbarButton>
      <ToolbarButton name="orderedList" active={editor.isActive('orderedList')} title="Numbered list">
        <ListOrdered className="h-4 w-4" aria-hidden />
      </ToolbarButton>
      <ToolbarButton name="blockquote" active={editor.isActive('blockquote')} title="Quote">
        <Quote className="h-4 w-4" aria-hidden />
      </ToolbarButton>
    </div>
  );
}
