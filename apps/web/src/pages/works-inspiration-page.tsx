/**
 * Inspiration Pool page — `/works/inspiration` (v1.222 P1 T2).
 *
 * Owns every hook, mutation, navigation and toast. The presentational chrome
 * under `components/works/presentational` only renders props, so mutation
 * failures are surfaced here (the query hooks deliberately carry no toast).
 *
 * Creator-scoped: none of the four payloads carries a `work_id`, so this is a
 * sibling route of `works/chapters`, outside `WorkShellLayout`. First page only
 * (`DEFAULT_PAGE_SIZE`) — upgrade to offset paging when a pool can exceed one
 * page. Promote is an atomic Work create server-side, so it lands on the new
 * Work's Outline and invalidates `works.lists()` via the query hook.
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useNavigate } from 'react-router';
import { ArrowLeft } from 'lucide-react';

import {
  DEFAULT_PAGE_SIZE,
  useAddWorkInspiration,
  useArchiveWorkInspiration,
  useErrorToast,
  usePromoteWorkInspiration,
  useWorkInspiration,
} from '@/api/queries';
import {
  InspirationPoolChrome,
  type InspirationPoolChromeState,
  type InspirationPoolCopy,
  type InspirationPoolRow,
  type InspirationPoolWorkLinkArgs,
} from '@/components/works/presentational/inspiration-pool-chrome';
import { formatDate } from '@/lib/format';

const STATUS_LABEL_KEYS: Readonly<Record<string, string>> = {
  idea: 'inspiration.statusIdea',
  promoted: 'inspiration.statusPromoted',
  archived: 'inspiration.statusArchived',
};

function renderWorkLink({ workId, className, children }: InspirationPoolWorkLinkArgs) {
  return (
    <Link to={`/works/${encodeURIComponent(workId)}/outline`} className={className}>
      {children}
    </Link>
  );
}

export function WorksInspirationPage() {
  const { t } = useTranslation('works');
  const { t: tCommon } = useTranslation('common');
  const navigate = useNavigate();
  const errorToast = useErrorToast();

  const [title, setTitle] = useState('');
  /**
   * The pool's one in-flight promote. The mutation hook's own `variables` only
   * tracks the LATEST call, so two overlapping promotes re-enabled row A while
   * B was pending and the server (which checks status once) could mint two
   * Works from one idea. Own the lock here instead: the first promote holds it
   * until it settles, and every promote button is disabled while held.
   */
  const [promotingItemId, setPromotingItemId] = useState<string | null>(null);
  const inspiration = useWorkInspiration({ limit: DEFAULT_PAGE_SIZE });
  const addMutation = useAddWorkInspiration();
  const promoteMutation = usePromoteWorkInspiration();
  const archiveMutation = useArchiveWorkInspiration();

  const items = inspiration.data?.items ?? [];

  const copy: InspirationPoolCopy = {
    title: t('inspiration.title'),
    description: t('inspiration.description'),
    listAriaLabel: t('inspiration.title'),
    loadingLabel: t('inspiration.loading'),
    emptyTitle: t('inspiration.emptyTitle'),
    emptyDescription: t('inspiration.emptyDescription'),
    errorTitle: tCommon('error.title'),
    errorDescription: t('inspiration.errorDescription'),
    retryLabel: tCommon('error.retry'),
    addTitle: t('inspiration.addTitle'),
    titleLabel: t('inspiration.titleLabel'),
    titlePlaceholder: t('inspiration.titlePlaceholder'),
    submitLabel: t('inspiration.submit'),
    submittingLabel: t('inspiration.submitting'),
    addedLabel: t('inspiration.addedLabel'),
    openWork: t('inspiration.openWork'),
    promoteLabel: t('inspiration.promote'),
    promotingLabel: t('inspiration.promoting'),
    archiveLabel: t('inspiration.archive'),
    archivingLabel: t('inspiration.archiving'),
  };

  const rows: InspirationPoolRow[] = items.map((item) => {
    const statusKey = STATUS_LABEL_KEYS[item.status];
    return {
      itemId: item.item_id,
      title: item.title,
      status: item.status,
      // An unexpected status renders verbatim rather than failing the row.
      statusLabel: statusKey ? t(statusKey) : item.status,
      addedText: formatDate(item.created_at),
      workId: item.promoted_work_id,
    };
  });

  const state: InspirationPoolChromeState = inspiration.isLoading
    ? 'loading'
    : inspiration.isError
      ? 'error'
      : rows.length === 0
        ? 'empty'
        : 'ready';

  function handleAdd(value: string) {
    addMutation.mutate(value, {
      onSuccess: () => setTitle(''),
      onError: (error) => errorToast(error, 'error.requestFailed'),
    });
  }

  function handlePromote(row: InspirationPoolRow) {
    // Disabled buttons cannot fire, but the guard is the invariant: one
    // promote at a time, whichever row asked.
    if (promotingItemId !== null) return;
    setPromotingItemId(row.itemId);
    promoteMutation.mutate(row.itemId, {
      onSuccess: (result) =>
        navigate(`/works/${encodeURIComponent(result.work_id)}/outline`),
      onError: (error) => errorToast(error, 'error.requestFailed'),
      onSettled: () => setPromotingItemId(null),
    });
  }

  function handleArchive(row: InspirationPoolRow) {
    archiveMutation.mutate(row.itemId, {
      onError: (error) => errorToast(error, 'error.requestFailed'),
    });
  }

  return (
    <div
      className="mx-auto flex w-full max-w-3xl flex-col gap-4 p-6"
      data-testid="works-inspiration-page"
    >
      <Link
        to="/works"
        className="inline-flex items-center gap-1.5 self-start text-label-14 text-gray-800 transition-colors duration-state ease-standard hover:text-gray-1000"
        data-testid="inspiration-back-link"
      >
        <ArrowLeft className="h-4 w-4" aria-hidden />
        {t('workDetail.backToWorks')}
      </Link>

      <InspirationPoolChrome
        copy={copy}
        state={state}
        rows={rows}
        total={inspiration.data?.total}
        onRetry={() => inspiration.refetch()}
        addTitleValue={title}
        onAddTitleChange={setTitle}
        onAdd={handleAdd}
        adding={addMutation.isPending}
        promotingItemId={promotingItemId}
        archivingItemId={archiveMutation.isPending ? archiveMutation.variables : null}
        onPromote={handlePromote}
        onArchive={handleArchive}
        renderWorkLink={renderWorkLink}
      />
    </div>
  );
}
