/**
 * ChaptersPage — V1.65 Content-Authoring structure table (T1 + T3).
 *
 * Per-Work chapter structure: chapter # / title (display-only) / slug /
 * planned word count / volume / status badge / actual word count. Inline edit
 * for slug / planned-wc / volume; status progression action `not_started →
 * outlined` only. Protected-chapter confirmation for `finalized`/`published`
 * structural edits; deletion hard-blocked. Multi-Work switcher reuses the Works
 * dashboard entry.
 */
import { useMemo, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router';
import { ArrowLeft, Check, FileText, Pencil, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { LoadMore } from '@/components/load-more';
import { ChapterStatusBadge } from '@/components/status-badge';
import { Button } from '@/components/ui/button';
import {
  Card, CardContent, CardDescription, CardHeader, CardTitle,
} from '@/components/ui/card';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import {
  Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from '@/components/ui/table';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import {
  flattenPages,
  useChapters,
  usePatchChapter,
  useWorks,
} from '@/api/queries';
import type { ChapterSummary, PatchChapterRequest } from '@42ch/nexus-contracts';

interface Edits {
  slug?: string;
  planned_word_count?: number;
  volume?: number;
}

export function ChaptersPage() {
  const { t } = useTranslation('memory');
  const { workId = '' } = useParams();
  const navigate = useNavigate();
  const works = useWorks();
  const chapters = useChapters(workId || undefined);
  const patch = usePatchChapter(workId || undefined);
  const rows = useMemo(() => flattenPages(chapters.data), [chapters.data]);

  const [editing, setEditing] = useState<number | null>(null);
  const [edits, setEdits] = useState<Edits>({});
  const [confirmChapter, setConfirmChapter] = useState<ChapterSummary | null>(null);

  function startEdit(row: ChapterSummary) {
    setEditing(row.chapter);
    setEdits({
      slug: row.slug ?? '',
      planned_word_count: row.planned_word_count,
      volume: row.volume,
    });
  }

  function cancelEdit() {
    setEditing(null);
    setEdits({});
  }

  async function saveEdit(row: ChapterSummary, confirmed = false) {
    const request: PatchChapterRequest = {
      slug: edits.slug,
      planned_word_count: edits.planned_word_count,
      volume: edits.volume,
    };
    // Only `finalized` is confirm-editable. `published` is server hard-blocked
    // (`CHAPTER_STRUCTURE_EDIT_BLOCKED`) with no `confirm_structural_edit`
    // override, so it must never reach an actionable confirm dialog — showing
    // one would always end in an error toast after the user confirms.
    if (row.status === 'finalized' && !confirmed) {
      setConfirmChapter(row);
      return;
    }
    if (row.status === 'finalized') {
      request.confirm_structural_edit = true;
    }
    await patch.mutateAsync({ chapter: row.chapter, request, query: { volume: row.volume ?? 1 } });
    setEditing(null);
    setEdits({});
  }

  async function advanceStatus(row: ChapterSummary) {
    if (row.status !== 'not_started') return;
    await patch.mutateAsync({
      chapter: row.chapter,
      request: { status: 'outlined' },
      query: { volume: row.volume ?? 1 },
    });
  }

  const workOptions = useMemo(() => flattenPages(works.data), [works.data]);

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Button asChild variant="tertiary" size="small">
            <Link to="/works"><ArrowLeft className="h-4 w-4" aria-hidden />{t('chapters.backToWorks')}</Link>
          </Button>
          {works.isLoading ? (
            <span className="text-copy-14 text-gray-700">{t('chapters.loadingWorks')}</span>
          ) : (
            <label className="flex items-center gap-2">
              <span className="sr-only">{t('chapters.selectWork')}</span>
              <select
                value={workId}
                onChange={(e) => navigate(`/works/${encodeURIComponent(e.target.value)}/chapters`)}
                className="h-9 rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000"
              >
                <option value="" disabled>{t('chapters.selectWorkPlaceholder')}</option>
                {workOptions.map((w) => (
                  <option key={w.work_id} value={w.work_id}>
                    {w.title || w.work_id}
                  </option>
                ))}
              </select>
            </label>
          )}
        </div>
      </div>

      <Card className="shadow-card">
        <CardHeader>
          <CardTitle>{t('chapters.title')}</CardTitle>
          <CardDescription>{t('chapters.description')}</CardDescription>
        </CardHeader>
        <CardContent>
          {!workId ? (
            <EmptyState
              title={t('chapters.noWorkTitle')}
              description={t('chapters.noWorkDescription')}
            />
          ) : chapters.isError ? (
            <ErrorState
              description={t('chapters.error')}
              onRetry={() => chapters.refetch()}
            />
          ) : chapters.isLoading ? (
            <LoadingState label={t('chapters.loading')} />
          ) : rows.length === 0 ? (
            <EmptyState
              title={t('chapters.noChaptersTitle')}
              description={t('chapters.noChaptersDescription')}
            />
          ) : (
            <>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="text-right">{t('chapters.columns.number')}</TableHead>
                    <TableHead>{t('chapters.columns.title')}</TableHead>
                    <TableHead>{t('chapters.columns.slug')}</TableHead>
                    <TableHead className="text-right">{t('chapters.columns.plannedWords')}</TableHead>
                    <TableHead className="text-right">{t('chapters.columns.volume')}</TableHead>
                    <TableHead>{t('chapters.columns.status')}</TableHead>
                    <TableHead className="text-right">{t('chapters.columns.actualWords')}</TableHead>
                    <TableHead className="text-right">{t('chapters.columns.actions')}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.map((row) => {
                    const isEditing = editing === row.chapter;
                    const isProtected = row.status === 'finalized' || row.status === 'published';
                    return (
                      <TableRow
                        key={row.chapter}
                        // V1.121 v0.4: protected-row tint converges onto the
                        // tokenized purple-700 scale (DESIGN.md
                        // components.data-table.row-protected intent —
                        // purple-700 low-alpha wash). Was a raw color-mix
                        // arbitrary class; now a Tailwind opacity modifier on
                        // the registered --color-purple-700 token (no new hue).
                        className={isProtected ? 'bg-purple-700/10' : undefined}
                        data-testid={`chapter-row-${row.chapter}`}
                      >
                        <TableCell className="text-right tabular-nums">{row.chapter}</TableCell>
                        <TableCell>
                          <Link
                            to={`/works/${encodeURIComponent(workId)}/chapters/${row.chapter}?volume=${row.volume ?? 1}`}
                            className="font-medium text-brand-deep-blue hover:underline dark:text-blue-700 dark:hover:text-blue-800"
                          >
                            {row.title?.trim() ? row.title : t('chapters.fallbackTitle', { chapter: row.chapter })}
                          </Link>
                        </TableCell>
                        <TableCell>
                          {isEditing ? (
                            <input
                              type="text"
                              value={edits.slug ?? ''}
                              onChange={(e) => setEdits((s) => ({ ...s, slug: e.target.value }))}
                              className="h-8 w-full rounded-control border border-blue-1000 bg-background-100 px-2 text-copy-14 dark:border-blue-700"
                              aria-label={t('chapters.slugLabel')}
                            />
                          ) : (
                            <span className="text-copy-13-mono text-gray-900">{row.slug?.trim() ? row.slug : '—'}</span>
                          )}
                        </TableCell>
                        <TableCell className="text-right">
                          {isEditing ? (
                            <input
                              type="number"
                              value={edits.planned_word_count ?? ''}
                              onChange={(e) =>
                                setEdits((s) => ({
                                  ...s,
                                  planned_word_count: e.target.value === '' ? undefined : Number(e.target.value),
                                }))
                              }
                              className="h-8 w-24 rounded-control border border-blue-1000 bg-background-100 px-2 text-right text-copy-14 tabular-nums dark:border-blue-700"
                              aria-label={t('chapters.plannedWordCountLabel')}
                            />
                          ) : (
                            <span className="tabular-nums">{row.planned_word_count.toLocaleString()}</span>
                          )}
                        </TableCell>
                        <TableCell className="text-right">
                          {isEditing ? (
                            <input
                              type="number"
                              value={edits.volume ?? ''}
                              onChange={(e) =>
                                setEdits((s) => ({
                                  ...s,
                                  volume: e.target.value === '' ? undefined : Number(e.target.value),
                                }))
                              }
                              className="h-8 w-16 rounded-control border border-blue-1000 bg-background-100 px-2 text-right text-copy-14 tabular-nums dark:border-blue-700"
                              aria-label={t('chapters.volumeLabel')}
                            />
                          ) : (
                            <span className="tabular-nums">{row.volume}</span>
                          )}
                        </TableCell>
                        <TableCell>
                          <ChapterStatusBadge status={row.status} />
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {row.actual_word_count?.toLocaleString() ?? '—'}
                        </TableCell>
                        <TableCell className="text-right">
                          <div className="flex items-center justify-end gap-1">
                            <Button
                              type="button"
                              variant="tertiary"
                              size="small"
                              asChild
                            >
                              <Link
                                to={`/works/${encodeURIComponent(workId)}/chapters/${row.chapter}?volume=${row.volume ?? 1}`}
                                aria-label={t('chapters.openAria', { chapter: row.chapter })}
                              >
                                <FileText className="h-4 w-4" aria-hidden />
                              </Link>
                            </Button>
                            {isEditing ? (
                              <>
                                <Button
                                  type="button"
                                  variant="tertiary"
                                  size="small"
                                  onClick={() => saveEdit(row)}
                                  aria-label={t('chapters.saveEdits')}
                                >
                                  <Check className="h-4 w-4" aria-hidden />
                                </Button>
                                <Button
                                  type="button"
                                  variant="tertiary"
                                  size="small"
                                  onClick={cancelEdit}
                                  aria-label={t('chapters.cancelEdits')}
                                >
                                  <X className="h-4 w-4" aria-hidden />
                                </Button>
                              </>
                            ) : row.status === 'published' ? (
                              // `published` chapters are server hard-blocked from
                              // structural edits (CHAPTER_STRUCTURE_EDIT_BLOCKED,
                              // no override). Render a disabled, informational
                              // affordance instead of an actionable Edit button.
                              <Button
                                type="button"
                                variant="tertiary"
                                size="small"
                                disabled
                                title={t('chapters.publishedTitle')}
                                aria-label={t('chapters.publishedDisabled')}
                              >
                                <Pencil className="h-4 w-4 opacity-disabled" aria-hidden />
                              </Button>
                            ) : (
                              <Button
                                type="button"
                                variant="tertiary"
                                size="small"
                                onClick={() => startEdit(row)}
                                aria-label={t('chapters.editStructure')}
                              >
                                <Pencil className="h-4 w-4" aria-hidden />
                              </Button>
                            )}
                            {row.status === 'not_started' && !isEditing && (
                              <Button
                                type="button"
                                variant="secondary"
                                size="small"
                                onClick={() => advanceStatus(row)}
                                disabled={patch.isPending}
                              >
                                {t('chapters.markOutlined')}
                              </Button>
                            )}
                          </div>
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>
              <LoadMore
                isFetchingNextPage={chapters.isFetchingNextPage}
                hasNextPage={chapters.hasNextPage}
                fetchNextPage={() => chapters.fetchNextPage()}
              />
            </>
          )}
        </CardContent>
      </Card>

      <ProtectedEditDialog
        chapter={confirmChapter}
        onCancel={() => setConfirmChapter(null)}
        onConfirm={() => {
          if (confirmChapter) {
            void saveEdit(confirmChapter, true);
          }
          setConfirmChapter(null);
        }}
      />
    </div>
  );
}

function ProtectedEditDialog({
  chapter,
  onCancel,
  onConfirm,
}: {
  chapter: ChapterSummary | null;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation('memory');
  return (
    <Dialog open={Boolean(chapter)} onOpenChange={(open) => !open && onCancel()}>
      {chapter && (
        <DialogContent
          title={t('chapters.dialog.title')}
          description={t('chapters.dialog.description', { status: chapter.status })}
        >
          <p className="text-copy-14 text-gray-900">
            {t('chapters.dialog.warning', { status: chapter.status })}
          </p>
          <div className="mt-4 flex justify-end gap-2">
            <Button type="button" variant="secondary" size="small" onClick={onCancel}>{t('chapters.dialog.cancel')}</Button>
            <Button type="button" variant="primary" size="small" onClick={onConfirm}>{t('chapters.dialog.confirm')}</Button>
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}
