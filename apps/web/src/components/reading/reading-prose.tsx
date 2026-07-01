/**
 * ReadingProse — V1.79 Author Reflection (Track A / P0).
 *
 * The prose reading surface: applies reading typography (measure, line-height,
 * paragraph spacing via DESIGN.md §Typography/reading-prose tokens) to the body
 * markdown render. Promotes the V1.75-pivot residuals verbatim — frontmatter
 * strip, ReactMarkdown + remark-gfm render, Copy Path affordance, and the
 * right-click PathContextMenu — so the reading value is preserved while the
 * typography becomes book-like. Read-only: no body mutation path exists here.
 */
import { useMemo } from 'react';
import { Copy } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

import { PathContextMenu, useContextMenu } from '@/components/path-context-menu';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useToast } from '@/lib/use-toast';
import type { ChapterBody } from '@42ch/nexus-contracts';

interface ReadingProseProps {
  body: ChapterBody | undefined;
  isLoading: boolean;
  isError: boolean;
  onRetry: () => void;
}

export function ReadingProse({ body, isLoading, isError, onRetry }: ReadingProseProps) {
  const { toast } = useToast();
  const menu = useContextMenu();

  const bodyContent = useMemo(() => stripFrontmatter(body), [body]);
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
    return <ErrorState description="Could not load the chapter body." onRetry={onRetry} />;
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
          <div className="reading-prose mx-auto max-w-[var(--reading-prose-measure)]">
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={proseRenderers}>
              {bodyContent}
            </ReactMarkdown>
          </div>
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

/**
 * Prose element renderers — apply reading typography (line-height + paragraph
 * spacing tokens) to body copy. Paragraphs get the comfortable book-like
 * line-height and inter-paragraph breathing room; headings keep the UI type
 * scale so chapter prose hierarchy still reads as part of the app.
 *
 * Inline styles read the DESIGN.md §Typography/reading-prose CSS vars so the
 * values stay SSOT-driven (not hard-coded in the component).
 */
const proseRenderers = {
  p: (props: React.HTMLAttributes<HTMLParagraphElement>) => (
    <p
      style={{
        lineHeight: 'var(--reading-prose-line-height)',
        marginTop: 'var(--reading-prose-paragraph-spacing)',
      }}
      {...props}
    />
  ),
};

function stripFrontmatter(body: ChapterBody | undefined): string {
  if (!body) return '';
  // If the API already separated frontmatter into `body.frontmatter`, the
  // `content` is already clean — return it directly. Calling the strip here
  // would double-strip (or mis-strip) content once the server populates
  // `frontmatter`.
  if (body.frontmatter && Object.keys(body.frontmatter).length > 0) {
    return body.content;
  }
  const content = body.content;
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
