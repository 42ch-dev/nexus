/**
 * ChapterPage — V1.75 Canvas-Pivot (A5 retire).
 *
 * The V1.65 whole-document TipTap outline editor has been RETIRED. Outline
 * authoring now happens on the V1.72 node-granular canvas (see the "Edit
 * outline → Canvas" CTA). This page is now a **read-only chapter reading /
 * preview view**: the body prose render (ReactMarkdown + remark-gfm with
 * frontmatter strip), the Copy Path affordance, and the right-click context
 * menu are preserved verbatim — the reading/preview value is unchanged.
 *
 * Removed in V1.75: the TipTap outline editor + toolbar, `usePutChapterOutline`
 * PUT save path, save-state indicator, save/reset buttons, the soft-concurrency
 * banner, the protected-edit banner, the Tabs wrapper, and `useChapterOutline`.
 */
import { useMemo } from 'react';
import { Link, useParams, useSearchParams } from 'react-router-dom';
import { ArrowLeft, ArrowUpRight, Copy } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

import { ChapterStatusBadge } from '@/components/status-badge';
import { PathContextMenu, useContextMenu } from '@/components/path-context-menu';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useChapter, useChapterBody } from '@/api/queries';
import { useToast } from '@/lib/use-toast';
import { formatRelative } from '@/lib/format';
import type { ChapterBody } from '@42ch/nexus-contracts';

export function ChapterPage() {
  const { workId = '', chapter: chapterParam = '' } = useParams();
  const chapterNumber = Number(chapterParam);
  const [searchParams] = useSearchParams();
  const volumeQuery = useMemo(() => {
    const raw = searchParams.get('volume');
    const n = raw === null ? undefined : Number(raw);
    return n !== undefined && n > 0 ? { volume: n } : undefined;
  }, [searchParams]);

  const chapter = useChapter(workId || undefined, chapterNumber || undefined, volumeQuery);
  const body = useChapterBody(workId || undefined, chapterNumber || undefined, volumeQuery);

  if (chapter.isLoading) {
    return <LoadingState label="Loading chapter…" />;
  }
  if (chapter.isError || !chapter.data) {
    return (
      <ErrorState
        description="Could not load this chapter. It may not exist or the daemon could not return it."
        onRetry={() => void chapter.refetch()}
      />
    );
  }

  const ch = chapter.data;
  const canvasHref = `/works/${encodeURIComponent(workId)}/outline?chapter=${ch.chapter}`;

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
          Updated {formatRelative(ch.updated_at)}
        </div>
      </div>

      <Card className="shadow-card">
        <CardHeader className="pb-3">
          <CardTitle>Outline editing moved to Canvas</CardTitle>
          <CardDescription>
            The whole-document outline editor was retired in V1.75. Edit this chapter&rsquo;s outline
            on the outline canvas.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button asChild variant="primary" size="small">
            <Link
              to={canvasHref}
              aria-label={`Edit outline for Chapter ${ch.chapter} on the outline canvas`}
            >
              Edit outline → Canvas <ArrowUpRight className="h-4 w-4" aria-hidden />
            </Link>
          </Button>
        </CardContent>
      </Card>

      <BodyReadOnly
        body={body.data}
        isLoading={body.isLoading}
        isError={body.isError}
        onRetry={() => body.refetch()}
      />
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
