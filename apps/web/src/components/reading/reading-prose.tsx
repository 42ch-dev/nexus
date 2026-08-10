/**
 * ReadingProse — V1.79 Author Reflection (Track A / P0) + V1.91 Profile-specific
 * Reading Chrome (P0 headline).
 *
 * The prose reading surface: applies reading typography (measure, line-height,
 * paragraph spacing via DESIGN.md §Typography/reading-prose tokens) to the body
 * markdown render. Promotes the V1.75-pivot residuals verbatim — frontmatter
 * strip, ReactMarkdown + remark-gfm render, Copy Path affordance, and the
 * right-click PathContextMenu — so the reading value is preserved while the
 * typography becomes book-like. Read-only: no body mutation path exists here.
 *
 * V1.91 adds profile-aware chrome. The chrome key (`novel`, `essay`,
 * `game-bible`, `script`) is derived from the Work's `work_profile` and
 * consumed exclusively through DESIGN.md `reading-chrome-*` tokens. Unknown or
 * missing profiles fall back to `novel` chrome.
 */
import { useMemo, type Ref } from 'react';
import { Copy } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { useTranslation } from 'react-i18next';

import { PathContextMenu, useContextMenu } from '@/components/path-context-menu';
import { createProfileRenderers } from '@/components/reading/reading-chrome-renderers';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useToast } from '@/lib/use-toast';
import { toReadingChromeProfile } from '@/lib/reading-chrome';
import type { ChapterBody } from '@42ch/nexus-contracts';

export interface ReadingProseProps {
  body: ChapterBody | undefined;
  isLoading: boolean;
  isError: boolean;
  onRetry: () => void;
  workProfile?: string;
  /** DOM ref forwarded to the prose surface (React 19 ref-as-prop). */
  ref?: Ref<HTMLDivElement>;
}

export function ReadingProse({ body, isLoading, isError, onRetry, workProfile, ref }: ReadingProseProps) {
  const { t } = useTranslation('reading');
  const { t: commonT } = useTranslation('common');
  const { toast } = useToast();
  const menu = useContextMenu();

  const bodyContent = useMemo(() => stripFrontmatter(body), [body]);
  const path = body?.body_path ?? '';

  const profile = useMemo(() => toReadingChromeProfile(workProfile), [workProfile]);
  const renderers = useMemo(() => createProfileRenderers(profile), [profile]);

  async function copyPath() {
    try {
      await navigator.clipboard.writeText(path);
      toast({ variant: 'success', title: commonT('toast.pathCopied') });
    } catch {
      toast({
        variant: 'error',
        title: commonT('toast.pathNotCopied'),
        description: commonT('toast.pathNotCopiedDescription'),
      });
    }
  }

  if (isLoading) return <LoadingState label={t('loading.body')} />;
  if (isError || !body) {
    return <ErrorState description={t('error.bodyLoadDescription')} onRetry={onRetry} />;
  }

  return (
    <Card className="shadow-card">
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <CardTitle>{t('prose.bodyTitle')}</CardTitle>
            <CardDescription>{t('prose.bodyDescription')}</CardDescription>
          </div>
          <Button
            type="button"
            variant="secondary"
            size="small"
            onClick={copyPath}
            aria-label={t('prose.copyPathAria')}
          >
            <Copy className="h-4 w-4" aria-hidden />{t('prose.copyPath')}
          </Button>
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-2 text-copy-13-mono text-gray-900">
          <span>{t('prose.pathLabel', { path: body.body_path })}</span>
          {Boolean(body.frontmatter?.status) && (
            <span className="rounded-pill border border-gray-alpha-300 px-2 py-0.5 text-label-12">
              {t('prose.statusLabel', { status: String(body.frontmatter!.status) })}
            </span>
          )}
        </div>
      </CardHeader>
      <CardContent>
        <div
          ref={ref}
          onContextMenu={menu.openMenu}
          className="rounded-card border border-gray-alpha-400 bg-background-100 p-6"
          role="region"
          aria-label={t('prose.bodyAriaLabel')}
          data-chrome-profile={profile}
        >
          <div className="reading-prose mx-auto max-w-[var(--reading-prose-measure)]">
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={renderers}>
              {bodyContent}
            </ReactMarkdown>
          </div>
        </div>
      </CardContent>

      {menu.open && (
        <PathContextMenu
          path={path}
          pathLabel={t('prose.bodyTitle')}
          position={menu.position}
          onClose={menu.close}
          regionLabel={t('prose.bodyAriaLabel')}
        />
      )}
    </Card>
  );
}
ReadingProse.displayName = 'ReadingProse';

function stripFrontmatter(body: ChapterBody | undefined): string {
  if (!body) return '';
  if (body.frontmatter && Object.keys(body.frontmatter).length > 0) {
    return body.content;
  }
  const content = body.content;
  const trimmed = content.trimStart();
  if (!trimmed.startsWith('---')) return content;
  const match = /\n---[ \t]*(?:\r?\n|$)/.exec(trimmed);
  if (!match) return content;
  return trimmed.slice(match.index + match[0].length);
}
