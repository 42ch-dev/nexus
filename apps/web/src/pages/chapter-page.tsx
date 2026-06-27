/**
 * ChapterPage — V1.65 Content-Authoring detail (T2 + T4).
 *
 * Loads a single chapter: outline rich-text editor (TipTap) with save-state
 * indicator and soft-concurrency warning banner; body read-only render via
 * react-markdown + remark-gfm with frontmatter header strip; right-click
 * context menu offering "Copy path" only.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Link, useParams, useSearchParams } from 'react-router-dom';
import {
  ArrowLeft,
  Bold,
  Copy,
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
import { EditorContent, useEditor } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import { Markdown } from 'tiptap-markdown';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

import { ChapterStatusBadge } from '@/components/status-badge';
import { PathContextMenu, useContextMenu } from '@/components/path-context-menu';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  useChapter,
  useChapterBody,
  useChapterOutline,
  usePutChapterOutline,
} from '@/api/queries';
import { useToast } from '@/lib/use-toast';
import { formatRelative } from '@/lib/format';
import type { ChapterBody } from '@42ch/nexus-contracts';

type SaveState = 'clean' | 'dirty' | 'saving' | 'saved-error';

export function ChapterPage() {
  const { workId = '', chapter: chapterParam = '' } = useParams();
  const chapterNumber = Number(chapterParam);
  // Thread the chapter's volume (from the list link's ?volume=N) through every
  // chapter-content hook. Without this, volume>1 chapters 404 because the
  // server defaults the volume query param to 1.
  const [searchParams] = useSearchParams();
  const volumeQuery = useMemo(() => {
    const raw = searchParams.get('volume');
    const n = raw === null ? undefined : Number(raw);
    return n !== undefined && n > 0 ? { volume: n } : undefined;
  }, [searchParams]);
  const chapter = useChapter(workId || undefined, chapterNumber || undefined, volumeQuery);
  const outline = useChapterOutline(workId || undefined, chapterNumber || undefined, volumeQuery);
  const body = useChapterBody(workId || undefined, chapterNumber || undefined, volumeQuery);
  const putOutline = usePutChapterOutline(workId || undefined, chapterNumber || undefined, volumeQuery);

  const [activeTab, setActiveTab] = useState('outline');
  const [saveState, setSaveState] = useState<SaveState>('clean');
  const [saveError, setSaveError] = useState<string | null>(null);
  const [lastSavedAt, setLastSavedAt] = useState<string | null>(null);
  // V1.66 desktop right-click menu on the outline editor surface
  // (web-ui-design-requirements §6.4). Shares the same component as the body
  // view; acts on the chapter's outline_path.
  const outlineMenu = useContextMenu();

  // Pin TipTap editor dependencies so `useEditor` does not re-initialize the
  // ProseMirror instance on every render (qc3 S-1).
  const editorExtensions = useMemo(() => [StarterKit, Markdown], []);
  const handleEditorUpdate = useCallback(() => {
    setSaveState('dirty');
    setSaveError(null);
  }, []);

  const editor = useEditor({
    extensions: editorExtensions,
    content: outline.data?.content ?? '',
    editable: true,
    onUpdate: handleEditorUpdate,
  });

  // Reset editor when outline loads or changes externally.
  useEffect(() => {
    if (editor && outline.data && !outline.isFetching) {
      const current = (editor.storage.markdown as { getMarkdown: () => string }).getMarkdown();
      if (current !== outline.data.content) {
        editor.commands.setContent(outline.data.content);
        setSaveState('clean');
        setSaveError(null);
        setLastSavedAt(outline.data.updated_at);
      }
    }
  }, [editor, outline.data, outline.isFetching]);

  async function handleSave() {
    if (!editor) return;
    const content = (editor.storage.markdown as { getMarkdown: () => string }).getMarkdown();
    setSaveState('saving');
    setSaveError(null);
    try {
      const result = await putOutline.mutateAsync({ content });
      setSaveState('clean');
      setLastSavedAt(result.updated_at);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Save failed.';
      setSaveState('saved-error');
      setSaveError(message);
    }
  }

  function handleReset() {
    if (!editor || !outline.data) return;
    editor.commands.setContent(outline.data.content);
    setSaveState('clean');
    setSaveError(null);
  }

  const showSoftConcurrency = chapter.data?.status === 'draft' || chapter.data?.status === 'finalized';

  if (chapter.isLoading || outline.isLoading) {
    return <LoadingState label="Loading chapter…" />;
  }
  if (chapter.isError || outline.isError || !chapter.data) {
    return (
      <ErrorState
        description="Could not load this chapter. It may not exist or the daemon could not return it."
        onRetry={() => {
          void chapter.refetch();
          void outline.refetch();
        }}
      />
    );
  }

  const ch = chapter.data;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Button asChild variant="tertiary" size="small">
            <Link to={`/works/${encodeURIComponent(workId)}/chapters`}>
              <ArrowLeft className="h-4 w-4" aria-hidden />Back to Chapters
            </Link>
          </Button>
          <span className="text-heading-20 font-heading tracking-tight text-gray-1000">
            Chapter {ch.chapter}
          </span>
          <ChapterStatusBadge status={ch.status} />
        </div>
        <div className="text-copy-13 text-gray-700">
          Updated {lastSavedAt ? formatRelative(lastSavedAt) : formatRelative(ch.updated_at)}
        </div>
      </div>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value="outline">Outline</TabsTrigger>
          <TabsTrigger value="body">Body</TabsTrigger>
        </TabsList>

        <TabsContent value="outline" className="mt-4">
          <Card className="shadow-card">
            <CardHeader className="pb-0">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <CardTitle>Outline</CardTitle>
                  <CardDescription>Plan the chapter's shape in markdown.</CardDescription>
                </div>
                <SaveStateIndicator state={saveState} error={saveError} />
              </div>
            </CardHeader>
            <CardContent className="pt-4">
              {showSoftConcurrency && <SoftConcurrencyBanner />}
              <div className="mt-2 overflow-hidden rounded-card border border-gray-alpha-400 bg-background-100 focus-within:border-blue-700 focus-within:ring-2 focus-within:ring-blue-700/20">
                <EditorToolbar editor={editor} />
                <div
                  onContextMenu={outlineMenu.openMenu}
                  className="min-h-[360px] p-6"
                  role="region"
                  aria-label="Chapter outline editor"
                >
                  <EditorContent editor={editor} className="prose prose-sm max-w-none text-copy-16 text-gray-1000" />
                </div>
                <div className="flex items-center justify-between border-t border-gray-alpha-400 bg-background-200 px-3 py-2">
                  <span className="text-copy-13 text-gray-900">Body writing is read-only in this version.</span>
                  <div className="flex items-center gap-2">
                    <Button
                      type="button"
                      variant="tertiary"
                      size="small"
                      onClick={handleReset}
                      disabled={saveState === 'clean'}
                    >
                      <RotateCcw className="h-4 w-4" aria-hidden />Reset
                    </Button>
                    <Button
                      type="button"
                      variant="primary"
                      size="small"
                      onClick={handleSave}
                      disabled={saveState !== 'dirty' && saveState !== 'saved-error'}
                    >
                      {saveState === 'saving' && <Loader2 className="h-4 w-4 animate-spin" aria-hidden />}
                      Save Outline
                    </Button>
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>

          {outlineMenu.open && (
            <PathContextMenu
              path={outline.data?.outline_path ?? ch.outline_path ?? ''}
              pathLabel="Outline"
              position={outlineMenu.position}
              onClose={outlineMenu.close}
              regionLabel="Outline context menu"
            />
          )}
        </TabsContent>

        <TabsContent value="body" className="mt-4">
          <BodyReadOnly body={body.data} isLoading={body.isLoading} isError={body.isError} onRetry={() => body.refetch()} />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function SaveStateIndicator({ state, error }: { state: SaveState; error: string | null }) {
  const config: Record<SaveState, { dot: string; label: string }> = {
    clean: { dot: 'bg-green-700', label: 'Saved' },
    dirty: { dot: 'bg-amber-700', label: 'Unsaved changes' },
    saving: { dot: 'bg-amber-700', label: 'Saving…' },
    'saved-error': { dot: 'bg-red-700', label: error ?? 'Save failed' },
  };
  const { dot, label } = config[state];
  return (
    <div className="flex items-center gap-2 text-label-12 text-gray-900" aria-live="polite">
      <span className={`h-2 w-2 rounded-pill ${dot}`} aria-hidden />
      <span className="max-w-[200px] truncate">{label}</span>
    </div>
  );
}

function SoftConcurrencyBanner() {
  return (
    <div className="mb-4 flex items-start gap-3 rounded-card border border-amber-700/30 bg-[color-mix(in_srgb,var(--color-amber-700)_8%,transparent)] p-4 text-copy-14 text-amber-1000">
      <span aria-hidden className="mt-0.5">⚠️</span>
      <div>
        <p className="font-medium">This chapter already has a draft body.</p>
        <p className="mt-1">
          Editing the outline will not re-draft it — the orchestration engine re-drafts only when the chapter transitions to draft status. To trigger a re-draft after saving: reverse the chapter status to outlined in the structure table, then advance it back to draft.
        </p>
      </div>
    </div>
  );
}

function EditorToolbar({ editor }: { editor: ReturnType<typeof useEditor> }) {
  if (!editor) return null;
  const toggle = (name: string) => {
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
      aria-pressed={active}
      className={`flex h-8 w-8 items-center justify-center rounded-control text-gray-1000 transition-colors duration-state ease-standard ${
        active ? 'bg-gray-alpha-200' : 'hover:bg-gray-alpha-100'
      }`}
    >
      {children}
    </button>
  );
  return (
    <div className="flex flex-wrap items-center gap-1 border-b border-gray-alpha-400 bg-background-200 px-2 py-1">
      <ToolbarButton name="heading1" active={editor.isActive('heading', { level: 1 })} title="Heading 1">
        <Heading1 className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton name="heading2" active={editor.isActive('heading', { level: 2 })} title="Heading 2">
        <Heading2 className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton name="heading3" active={editor.isActive('heading', { level: 3 })} title="Heading 3">
        <Heading3 className="h-4 w-4" />
      </ToolbarButton>
      <span className="mx-1 h-5 w-px bg-gray-alpha-400" />
      <ToolbarButton name="bold" active={editor.isActive('bold')} title="Bold">
        <Bold className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton name="italic" active={editor.isActive('italic')} title="Italic">
        <Italic className="h-4 w-4" />
      </ToolbarButton>
      <span className="mx-1 h-5 w-px bg-gray-alpha-400" />
      <ToolbarButton name="bulletList" active={editor.isActive('bulletList')} title="Bullet list">
        <List className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton name="orderedList" active={editor.isActive('orderedList')} title="Numbered list">
        <ListOrdered className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton name="blockquote" active={editor.isActive('blockquote')} title="Quote">
        <Quote className="h-4 w-4" />
      </ToolbarButton>
    </div>
  );
}

function BodyReadOnly({
  body,
  isLoading,
  isError,
  onRetry,
}: {
  body: ChapterBody | undefined;
  isLoading: boolean;
  isError: boolean;
  onRetry: () => void;
}) {
  const { toast } = useToast();
  const menu = useContextMenu();

  const bodyContent = useMemo(() => {
    if (!body) return '';
    // If the API already separated frontmatter into `body.frontmatter`, the
    // `content` is already clean — return it directly. Calling stripFrontmatter
    // here would double-strip (or mis-strip) content once the server populates
    // `frontmatter`.
    if (body.frontmatter && Object.keys(body.frontmatter).length > 0) {
      return body.content;
    }
    return stripFrontmatter(body.content);
  }, [body]);

  const path = body?.body_path ?? '';

  async function copyPath() {
    try {
      await navigator.clipboard.writeText(path);
      toast({ variant: 'success', title: 'Path copied' });
    } catch {
      toast({
        variant: 'error',
        title: 'Path not copied',
        description: 'Copy it manually from the details panel.',
      });
    }
  }

  if (isLoading) return <LoadingState label="Loading body…" />;
  if (isError || !body) {
    return (
      <ErrorState
        description="Could not load the chapter body."
        onRetry={onRetry}
      />
    );
  }

  return (
    <Card className="shadow-card">
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <CardTitle>Body</CardTitle>
            <CardDescription>Read-only render of the drafted chapter body.</CardDescription>
          </div>
          <Button
            type="button"
            variant="secondary"
            size="small"
            onClick={copyPath}
            aria-label="Copy body path"
          >
            <Copy className="h-4 w-4" aria-hidden />Copy Path
          </Button>
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-2 text-copy-13-mono text-gray-900">
          <span>Path: {body.body_path}</span>
          {Boolean(body.frontmatter?.status) && (
            <span className="rounded-pill border border-gray-alpha-300 px-2 py-0.5 text-label-12">
              status: {String(body.frontmatter!.status)}
            </span>
          )}
        </div>
      </CardHeader>
      <CardContent>
        <div
          onContextMenu={menu.openMenu}
          className="rounded-card border border-gray-alpha-400 bg-background-100 p-6"
          role="region"
          aria-label="Chapter body"
        >
          <ReactMarkdown remarkPlugins={[remarkGfm]} className="prose prose-sm max-w-none text-copy-16 text-gray-1000">
            {bodyContent}
          </ReactMarkdown>
        </div>
      </CardContent>

      {menu.open && (
        <PathContextMenu
          path={path}
          pathLabel="Body"
          position={menu.position}
          onClose={menu.close}
          regionLabel="Body context menu"
        />
      )}
    </Card>
  );
}

function stripFrontmatter(content: string): string {
  const trimmed = content.trimStart();
  if (!trimmed.startsWith('---')) return content;
  // The closing fence must appear at the beginning of a line (standard YAML
  // front matter). A plain indexOf('---', 3) would match `---` embedded inside
  // a YAML value such as `title: foo --- bar` and strip at the wrong offset,
  // garbling the body handed to ReactMarkdown.
  const match = /\n---[ \t]*(?:\r?\n|$)/.exec(trimmed);
  if (!match) return content;
  return trimmed.slice(match.index + match[0].length);
}
