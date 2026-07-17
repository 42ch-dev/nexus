import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useMatch, useParams } from 'react-router-dom';
import { BookOpen } from 'lucide-react';

import { flattenPages, useWork, useWorks } from '@/api/queries';
import { StatusBadge } from '@/components/status-badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { formatRelative, humanizeStatus, shortId } from '@/lib/format';
import { cn } from '@/lib/utils';

export interface WorkRailProps {
  /** Called after navigating to another work (e.g. close mobile drawer). */
  onWorkSelect?: () => void;
  /** When false, omits the rail title row (drawer supplies its own). */
  showHeader?: boolean;
}

/**
 * Right rail for the canvas-first work shell — Works list + metadata preview.
 *
 * Uses the same `useWorks({ limit: 12 })` source as the P1 sidebar Works
 * group. Row click navigates to `/works/:id/outline` without a full reload.
 */
export function WorkRail({ onWorkSelect, showHeader = true }: WorkRailProps) {
  const { t } = useTranslation('shell');
  const { workId = '' } = useParams<{ workId?: string }>();
  const worksQuery = useWorks({ limit: 12 });
  const works = useMemo(() => flattenPages(worksQuery.data), [worksQuery.data]);

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="work-rail">
      {showHeader ? (
        <div className="border-b border-gray-alpha-400 px-4 py-3">
          <h2 className="text-label-14 font-medium text-gray-1000">{t('workShell.railTitle')}</h2>
        </div>
      ) : null}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {worksQuery.isLoading ? (
          <LoadingState label={t('workShell.loadingWorks')} />
        ) : worksQuery.isError ? (
          <ErrorState
            title={t('workShell.worksErrorTitle')}
            description={t('workShell.worksErrorDescription')}
            onRetry={() => worksQuery.refetch()}
          />
        ) : works.length === 0 ? (
          <p className="px-4 py-6 text-copy-14 text-gray-700">{t('workShell.noWorks')}</p>
        ) : (
          <ul className="flex flex-col gap-0.5 p-2" aria-label={t('workShell.worksListAria')}>
            {works.map((work) => (
              <WorkRailListItem
                key={work.work_id}
                workId={work.work_id}
                title={work.title || t('workShell.untitled')}
                currentWorkId={workId}
                onWorkSelect={onWorkSelect}
              />
            ))}
          </ul>
        )}
      </div>

      <WorkRailPreview workId={workId} />
    </div>
  );
}

function WorkRailListItem({
  workId,
  title,
  currentWorkId,
  onWorkSelect,
}: {
  workId: string;
  title: string;
  currentWorkId: string;
  onWorkSelect?: () => void;
}) {
  const outlinePath = `/works/${encodeURIComponent(workId)}/outline`;
  const outlineMatch = useMatch({ path: outlinePath, end: false });
  const isOutlineActive = outlineMatch !== null;
  const isCurrentWork = workId === currentWorkId;

  return (
    <li>
      <Link
        to={outlinePath}
        onClick={() => onWorkSelect?.()}
        aria-current={isCurrentWork || isOutlineActive ? 'page' : undefined}
        data-testid={`work-rail-item-${workId}`}
        className={cn(
          'flex w-full items-center gap-2 rounded-control px-3 py-2 text-left text-label-14 transition-colors duration-state ease-standard motion-reduce:transition-none',
          isCurrentWork || isOutlineActive
            ? 'bg-gray-alpha-100 text-gray-1000'
            : 'text-gray-800 hover:bg-gray-alpha-100 hover:text-gray-1000',
        )}
      >
        <BookOpen className="h-4 w-4 shrink-0 text-gray-700" aria-hidden />
        <span className="min-w-0 flex-1 truncate">{title}</span>
      </Link>
    </li>
  );
}

/** Metadata-only preview card for the route-scoped work (no manuscript snippet). */
function WorkRailPreview({ workId }: { workId: string }) {
  const { t } = useTranslation('shell');
  const work = useWork(workId || undefined);

  if (!workId) return null;

  return (
    <div
      className="shrink-0 border-t border-gray-alpha-400 p-4"
      data-testid="work-rail-preview"
      aria-label={t('workShell.previewAria')}
    >
      {work.isLoading ? (
        <LoadingState label={t('workShell.loadingPreview')} />
      ) : work.isError || !work.data ? (
        <p className="text-copy-13 text-gray-700">{t('workShell.previewUnavailable')}</p>
      ) : (
        <Card className="shadow-card">
          <CardHeader className="gap-2 p-4 pb-2">
            <CardTitle className="text-heading-16">{work.data.title || t('workShell.untitled')}</CardTitle>
            <CardDescription>
              <span className="text-copy-13-mono">{shortId(work.data.work_id)}</span>
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-3 p-4 pt-0">
            <div className="flex flex-wrap gap-2">
              <StatusBadge status={work.data.status} />
              {work.data.work_profile ? (
                <StatusBadge status={work.data.work_profile} variant="preset" />
              ) : null}
            </div>
            <dl className="flex flex-col gap-2 text-copy-13 text-gray-900">
              <div className="flex flex-col gap-0.5">
                <dt className="text-label-12 text-gray-700">{t('workShell.previewPreset')}</dt>
                <dd className="text-copy-13-mono">{shortId(work.data.primary_preset_id)}</dd>
              </div>
              <div className="flex flex-col gap-0.5">
                <dt className="text-label-12 text-gray-700">{t('workShell.previewUpdated')}</dt>
                <dd>{formatRelative(work.data.updated_at)}</dd>
              </div>
              {work.data.work_profile ? (
                <div className="flex flex-col gap-0.5">
                  <dt className="text-label-12 text-gray-700">{t('workShell.previewProfile')}</dt>
                  <dd>{humanizeStatus(work.data.work_profile)}</dd>
                </div>
              ) : null}
            </dl>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
