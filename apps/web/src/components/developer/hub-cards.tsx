/**
 * Hub status cards (plan v1220 T2) — the four read-only sections of 操作台.
 *
 * Each card renders live data from existing daemon endpoints and owns its own
 * loading (`正在加载…`) / error (`加载失败：{reason}` + retry) state, so one
 * failing source never blanks the page. Cards are navigation-only: the sole
 * interactive elements are the `go` links and an error retry.
 */
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { HubCard } from '@/components/developer/hub-card';
import { Button } from '@/components/ui/button';
import { errorMessage } from '@/lib/error-message';
import {
  useAgentSummary,
  useHarnessSummary,
  useModulesSummary,
  useServiceSummary,
  type HubSectionState,
} from '@/lib/developer/hub-summary';

/** Section-scoped loading / error block; children render once data exists. */
function HubCardBody({
  id,
  state,
  children,
}: {
  id: string;
  state: HubSectionState<unknown>;
  children?: ReactNode;
}) {
  const { t } = useTranslation('shell');
  const { t: tCommon } = useTranslation('common');

  if (state.error) {
    return (
      <div
        className="flex flex-col items-start gap-2"
        data-testid={`hub-card-${id}-error`}
        role="alert"
      >
        <p className="text-copy-13 text-red-900">
          {t('hub.operating.error', {
            reason: errorMessage(state.error) || tCommon('error.requestFailed'),
          })}
        </p>
        <Button type="button" variant="tertiary" size="small" onClick={state.retry}>
          {tCommon('error.retry')}
        </Button>
      </div>
    );
  }

  if (state.isLoading) {
    return (
      <p className="text-copy-13 text-gray-700" data-testid={`hub-card-${id}-loading`}>
        {t('hub.operating.loading')}
      </p>
    );
  }

  return <>{children}</>;
}

function ServiceCard() {
  const { t } = useTranslation('shell');
  const summary = useServiceSummary();
  const degraded = (summary.data?.degradedSubsystems.length ?? 0) > 0;

  return (
    <HubCard
      id="service"
      title={t('hub.operating.service.title')}
      status={
        summary.data
          ? {
              label: degraded
                ? t('hub.operating.service.degraded')
                : t('hub.operating.service.running'),
              variant: degraded ? 'warning' : 'running',
            }
          : undefined
      }
    >
      <HubCardBody id="service" state={summary}>
        {summary.data && (
          <>
            <p
              className="text-copy-14 font-semibold text-gray-1000"
              data-testid="hub-card-service-version"
            >
              {t('hub.operating.service.version', { version: summary.data.version })}
            </p>
            <p className="text-copy-13 text-gray-700" data-testid="hub-card-service-url">
              {summary.data.entryUrl}
            </p>
            <p
              className="text-copy-13 text-gray-700"
              data-testid="hub-card-service-subsystems"
            >
              {degraded
                ? t('hub.operating.service.degradedSubsystems', {
                    subsystems: summary.data.degradedSubsystems.join(' / '),
                  })
                : t('hub.operating.service.subsystemsOk')}
            </p>
          </>
        )}
      </HubCardBody>
    </HubCard>
  );
}

function AgentCard() {
  const { t } = useTranslation('shell');
  const summary = useAgentSummary();
  const data = summary.data;

  return (
    <HubCard
      id="agent"
      title={t('hub.operating.agent.title')}
      status={
        data
          ? {
              label: data.connected
                ? t('hub.operating.agent.connected')
                : t('hub.operating.agent.notConnected'),
              variant: data.connected ? 'running' : 'warning',
            }
          : undefined
      }
      go={{ to: '/strategies', label: t('hub.operating.go.manage') }}
    >
      <HubCardBody id="agent" state={summary}>
        {data && (
          <>
            <p
              className="text-copy-14 font-semibold text-gray-1000"
              data-testid="hub-card-agent-name"
            >
              {data.displayName ?? t('hub.operating.agent.none')}
            </p>
            {data.version && (
              <p className="text-copy-13 text-gray-700" data-testid="hub-card-agent-version">
                {data.version}
              </p>
            )}
          </>
        )}
      </HubCardBody>
    </HubCard>
  );
}

function HarnessCard() {
  const { t } = useTranslation('shell');
  const summary = useHarnessSummary();

  return (
    <HubCard
      id="harness"
      title={t('hub.operating.harness.title')}
      go={{ to: '/strategies', label: t('hub.operating.go.enter') }}
    >
      <HubCardBody id="harness" state={summary}>
        {summary.data && (
          <p
            className="text-copy-14 font-semibold text-gray-1000"
            data-testid="hub-card-harness-counts"
          >
            {t('hub.operating.harness.counts', {
              count: summary.data.presetCount,
              running: summary.data.runningSessions,
            })}
          </p>
        )}
      </HubCardBody>
    </HubCard>
  );
}

function ModulesCard() {
  const { t } = useTranslation('shell');
  const summary = useModulesSummary();
  const names = summary.data?.names ?? [];

  return (
    <HubCard
      id="modules"
      title={t('hub.operating.modules.title')}
      go={{ to: '/modules', label: t('hub.operating.go.enter') }}
    >
      <HubCardBody id="modules" state={summary}>
        {summary.data && (
          <>
            <p
              className="text-copy-14 font-semibold text-gray-1000"
              data-testid="hub-card-modules-count"
            >
              {t('hub.operating.modules.count', { count: summary.data.count })}
            </p>
            <p className="text-copy-13 text-gray-700" data-testid="hub-card-modules-list">
              {names.length > 0
                ? names.join(' · ')
                : t('hub.operating.modules.empty')}
            </p>
          </>
        )}
      </HubCardBody>
    </HubCard>
  );
}

export function HubCards() {
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2" data-testid="hub-cards">
      <ServiceCard />
      <AgentCard />
      <HarnessCard />
      <ModulesCard />
    </div>
  );
}
