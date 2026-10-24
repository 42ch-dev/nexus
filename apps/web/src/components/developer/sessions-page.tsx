/**
 * Orchestration sessions view (Control Room — READ) — web-ui.md §6.1 #2 /
 * plan v221-caps-sessions-rail T2.
 *
 * Lists every orchestration session as a card: short id, status pill, preset
 * and creator. The endpoint is un-paginated and the client keeps the server's
 * order (F-F1 — `SessionSummary` carries no timestamp to sort by). `current_task_id`
 * is not rendered: it is a transient pointer already surfaced by the run view.
 *
 * v1.201 P1 (compass D7): each card opens the read-only run view
 * (`/sessions/:sessionId`), the plan's acceptance-bearing observation surface.
 * The markup lives in `components/developer/presentational/session-card.tsx`
 * (fixtured in the Design Studio gallery); this file is app wiring only.
 */
import { RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router';

import { useSessions } from '@/api/queries';
import {
  SessionCardChrome,
  SessionCardGrid,
  type SessionCardCopy,
  type SessionCardLinkArgs,
} from '@/components/developer/presentational/session-card';
import { statusVariant } from '@/components/status-badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState, UnavailableState } from '@/components/ui/states';
import { humanizeStatus, shortId } from '@/lib/format';
import { isOrchestrationEngineUnavailable } from '@/lib/nexus/errors';

/** `/sessions` page entry — the `session` rail app's `load` target. */
export function SessionsPage() {
  const { t } = useTranslation('sessions');
  const sessions = useSessions();

  const copy: SessionCardCopy = {
    openRunAria: (id) => t('openRunAria', { id }),
    presetLabel: t('columns.preset'),
    creatorLabel: t('columns.creator'),
  };

  return (
    <Card className="shadow-card">
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <div>
            <CardTitle>{t('title')}</CardTitle>
            <CardDescription>{t('description')}</CardDescription>
          </div>
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={() => sessions.refetch()}
            disabled={sessions.isFetching}
            aria-label={t('refreshAria')}
          >
            <RefreshCw className={`h-4 w-4 ${sessions.isFetching ? 'animate-spin' : ''}`} aria-hidden />
            {t('refresh')}
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {sessions.isError ? (
          isOrchestrationEngineUnavailable(sessions.error) ? (
            <UnavailableState
              title={t('engineUnavailableTitle')}
              description={t('engineUnavailableDescription')}
              onRetry={() => sessions.refetch()}
            />
          ) : (
            <ErrorState
              title={t('errorTitle')}
              description={t('errorDescription')}
              onRetry={() => sessions.refetch()}
            />
          )
        ) : sessions.isLoading ? (
          <LoadingState label={t('loading')} />
        ) : !sessions.data || sessions.data.length === 0 ? (
          <EmptyState title={t('emptyTitle')} description={t('emptyDescription')} />
        ) : (
          <SessionCardGrid>
            {sessions.data.map((s) => (
              <SessionCardChrome
                key={s.session_id}
                card={{
                  sessionId: s.session_id,
                  shortId: shortId(s.session_id),
                  // Locked v1.220 pill mapping — unchanged `statusVariant`,
                  // `completed` reads green and is told apart by its label.
                  status: {
                    label: humanizeStatus(s.status),
                    variant: statusVariant(s.status),
                  },
                  preset: shortId(s.preset_id),
                  creator: shortId(s.creator_id),
                }}
                copy={copy}
                renderLink={renderRunLink}
              />
            ))}
          </SessionCardGrid>
        )}
      </CardContent>
    </Card>
  );
}

/** The card is one link to the session's run view (compass D7). */
function renderRunLink({ href, ariaLabel, children }: SessionCardLinkArgs) {
  return (
    <Link
      to={href}
      aria-label={ariaLabel}
      className="block rounded-card focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
    >
      {children}
    </Link>
  );
}
