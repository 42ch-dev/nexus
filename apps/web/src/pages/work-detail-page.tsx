import { useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { ArrowLeft, Pencil } from 'lucide-react';

import { StatusBadge } from '@/components/status-badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useFindings, usePatchWork, useWork } from '@/api/queries';
import { formatDateTime, formatRelative, humanizeStatus, shortId } from '@/lib/format';
import { LoadMore } from '@/components/load-more';
import { flattenPages } from '@/api/queries';

import { PatchWorkDialog } from './dialogs/patch-work-dialog';

/**
 * Work detail (Control Room — READ) — web-ui.md §6.1 #1 drill-in.
 *
 * Shows intake status, current stage, world/preset binding, linked schedules,
 * and the chapter progress for novel-profile Works (derived from
 * `current_chapter` / `total_planned_chapters`). Includes a findings section
 * (F-P2 endpoint) and a status/stage patch entry point (Setup — T7).
 */
export function WorkDetailPage() {
  const { workId = '' } = useParams();
  const work = useWork(workId);
  const patch = usePatchWork();
  const [patchOpen, setPatchOpen] = useState(false);
  const [archiveConfirm, setArchiveConfirm] = useState(false);

  if (work.isLoading) return <LoadingState label="Loading Work…" />;
  if (work.isError || !work.data) {
    return (
      <ErrorState
        title="Work not found"
        description="This Work does not exist or the daemon could not return it."
        onRetry={() => work.refetch()}
      />
    );
  }

  const w = work.data;
  const isArchived = w.status.toLowerCase() === 'archived';
  const hasChapterPlan = typeof w.total_planned_chapters === 'number' && w.total_planned_chapters > 0;
  const completionPct = hasChapterPlan
    ? Math.min(100, Math.round((w.current_chapter / (w.total_planned_chapters ?? 1)) * 100))
    : null;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <Button asChild variant="tertiary" size="small">
          <Link to="/works"><ArrowLeft className="h-4 w-4" aria-hidden />Back to Works</Link>
        </Button>
      </div>

      <Card className="shadow-card">
        <CardHeader>
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="flex flex-col gap-1">
              <CardTitle>{w.title || '(untitled)'}</CardTitle>
              <CardDescription>
                <span className="text-copy-13-mono">{shortId(w.work_id)}</span>
              </CardDescription>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <StatusBadge status={w.status} />
              <StatusBadge status={w.work_profile ? humanizeStatus(w.work_profile) : undefined} variant="preset" />
              <Button type="button" variant="secondary" size="small" onClick={() => setPatchOpen(true)}>
                <Pencil className="h-4 w-4" aria-hidden />
                Update Work
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-1 gap-x-8 gap-y-4 md:grid-cols-2">
            <Detail label="Long-term goal" value={w.long_term_goal || '—'} />
            <Detail label="Initial idea" value={w.initial_idea || '—'} />
            <Detail label="Intake status"><StatusBadge status={w.intake_status} /></Detail>
            <Detail label="Current stage">{w.current_stage || '—'} <span className="text-gray-700">· {humanizeStatus(w.stage_status)}</span></Detail>
            <Detail label="Profile">{w.work_profile ? humanizeStatus(w.work_profile) : '—'}</Detail>
            <Detail label="Primary preset"><span className="text-copy-13-mono">{shortId(w.primary_preset_id)}</span></Detail>
            <Detail label="World"><span className="text-copy-13-mono">{shortId(w.world_id)}</span></Detail>
            <Detail label="Story ref">{w.story_ref || '—'}</Detail>
            <Detail label="Created">{formatDateTime(w.created_at)}</Detail>
            <Detail label="Updated">{formatRelative(w.updated_at)}</Detail>
          </dl>

          {(hasChapterPlan || w.work_profile === 'novel') && (
            <div className="mt-6 rounded-card border border-gray-alpha-400 bg-background-200 p-4">
              <p className="text-label-14 font-medium text-gray-1000">Chapter progress</p>
              {hasChapterPlan ? (
                <div className="mt-2 flex items-center gap-3">
                  <div className="h-2 flex-1 overflow-hidden rounded-pill bg-gray-alpha-300">
                    <div
                      className="h-full rounded-pill bg-blue-700"
                      style={{ width: `${completionPct}%` }}
                      role="progressbar"
                      aria-valuenow={completionPct ?? 0}
                      aria-valuemin={0}
                      aria-valuemax={100}
                    />
                  </div>
                  <span className="tabular-nums text-copy-13-mono text-gray-900">
                    {w.current_chapter}/{w.total_planned_chapters} ({completionPct}%)
                  </span>
                </div>
              ) : (
                <p className="mt-1 text-copy-13 text-gray-700">Chapter {w.current_chapter} (no planned total).</p>
              )}
            </div>
          )}

          <div className="mt-6 flex flex-wrap gap-2">
            {!isArchived && (
              <Button
                type="button"
                variant={archiveConfirm ? 'destructive' : 'tertiary'}
                size="small"
                onClick={async () => {
                  if (!archiveConfirm) {
                    setArchiveConfirm(true);
                    return;
                  }
                  await patch.mutateAsync({ workId: w.work_id, request: { status: 'archived' } });
                  setArchiveConfirm(false);
                }}
              >
                {archiveConfirm ? 'Confirm archive' : 'Archive Work'}
              </Button>
            )}
          </div>
        </CardContent>
      </Card>

      <FindingsSection workId={w.work_id} />

      <PatchWorkDialog work={w} open={patchOpen} onOpenChange={setPatchOpen} />
    </div>
  );
}

function Detail({ label, value, children }: { label: string; value?: string; children?: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <dt className="text-label-12 uppercase tracking-wide text-gray-700">{label}</dt>
      <dd className="text-copy-14 text-gray-1000">{value ?? children}</dd>
    </div>
  );
}

function FindingsSection({ workId }: { workId: string }) {
  const findings = useFindings(workId);
  const rows = flattenPages(findings.data);
  return (
    <Card className="shadow-card">
      <CardHeader>
        <CardTitle>Findings</CardTitle>
        <CardDescription>Findings raised against this Work, with severity filtering.</CardDescription>
      </CardHeader>
      <CardContent>
        {findings.isError ? (
          <ErrorState
            description="Could not load findings for this Work."
            onRetry={() => findings.refetch()}
          />
        ) : findings.isLoading ? (
          <LoadingState label="Loading findings…" />
        ) : rows.length === 0 ? (
          <p className="py-6 text-copy-14 text-gray-700">No findings recorded for this Work.</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {rows.map((f) => (
              <li key={f.finding_id} className="rounded-card border border-gray-alpha-400 p-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <p className="text-copy-14 font-medium text-gray-1000">{f.title || '(untitled finding)'}</p>
                  <div className="flex items-center gap-2">
                    <StatusBadge status={f.severity} variant={undefined} raw />
                    <StatusBadge status={f.status} />
                  </div>
                </div>
                {f.description && <p className="mt-1 text-copy-13 text-gray-900">{f.description}</p>}
                <p className="mt-1 text-copy-13-mono text-gray-700">
                  {shortId(f.finding_id)} · {humanizeStatus(f.kind)}
                </p>
              </li>
            ))}
            <LoadMore
              isFetchingNextPage={findings.isFetchingNextPage}
              hasNextPage={findings.hasNextPage}
              fetchNextPage={() => findings.fetchNextPage()}
              label="Load more findings"
            />
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
