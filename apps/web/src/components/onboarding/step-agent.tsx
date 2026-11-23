/**
 * Scan states: loading
 * (Next disabled), ready (rows + recommended preselected), empty and error
 * (recoverable through the install guide / manual launch command).
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { AgentRow, ManualCommandField, ScanErrorNotice, useAgentSelection } from '@/components/onboarding/agent-selection';
import { Button } from '@/components/ui';
import { resolveInstallUrl } from '@/lib/agent-catalog';
import { useDesktopCapabilities } from '@/lib/client-context';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

interface InstallTarget {
  label: string;
  url: string;
}

const INSTALL_TARGETS: InstallTarget[] = [
  { label: 'Claude Code', url: resolveInstallUrl('claude-native') },
  { label: 'Codex CLI', url: resolveInstallUrl('codex-native') },
].filter((target): target is InstallTarget => target.url !== null);

const LINK_CLASS =
  'self-start text-label-14 font-medium text-brand-deep-blue transition-colors duration-state ease-standard hover:opacity-80 dark:text-blue-700 dark:hover:opacity-100';

function InstallLink({ target, desktop }: { target: InstallTarget; desktop: DesktopCapabilities | null }) {
  if (!desktop) {
    return (
      <a href={target.url} target="_blank" rel="noopener noreferrer" className={LINK_CLASS}>
        {target.label}
      </a>
    );
  }
  return (
    <a
      href={target.url}
      onClick={(event) => {
        event.preventDefault();
        void desktop.openExternalUrl(target.url).catch(() => {
          console.error('Failed to open external URL:', target.url);
        });
      }}
      className={LINK_CLASS}
    >
      {target.label}
    </a>
  );
}

interface StepAgentProps {
  onBack: () => void;
  onNext: () => void;
}

export function StepAgent({ onBack, onNext }: StepAgentProps) {
  const { t } = useTranslation('setup');
  const selection = useAgentSelection();
  const desktop = useDesktopCapabilities();
  const [showInstallGuide, setShowInstallGuide] = useState(false);
  const [showManual, setShowManual] = useState(false);

  const scanning = selection.status === 'loading';
  const canContinue = !scanning && selection.canContinue;

  async function advance() {
    if (await selection.commit()) onNext();
  }

  return (
    <div className="flex flex-col items-center">
      <div className="text-label-12 text-gray-700">{t('wizard.agent.stepLabel')}</div>
      <h1 className="mt-4 font-display text-display-32 text-gray-1000">
        {selection.status === 'empty' ? t('wizard.agent.emptyTitle') : t('wizard.agent.title')}
      </h1>

      {scanning ? (
        <p role="status" className="mt-3 text-copy-14 text-gray-900">
          {t('wizard.scan.loading')}
        </p>
      ) : null}

      {selection.status === 'ready' ? (
        <p className="mt-3 text-copy-14 text-gray-900">
          {t('wizard.agent.found', { count: selection.choices.length })}
        </p>
      ) : null}

      {selection.status === 'empty' ? (
        <p className="mt-3 max-w-[480px] text-center text-copy-14 text-gray-900">
          {t('wizard.agent.emptyDescription')}
        </p>
      ) : null}

      {selection.status === 'error' ? (
        <div className="mt-3">
          <ScanErrorNotice reason={selection.errorReason} onRetry={selection.retry} />
        </div>
      ) : null}

      {selection.status === 'ready' ? (
        <div
          role="radiogroup"
          aria-label={t('wizard.agent.title')}
          className="mt-8 flex w-[480px] flex-col gap-3"
        >
          {selection.choices.map((choice) => (
            <AgentRow
              key={choice.pickerId}
              choice={choice}
              description={
                choice.pickerId === selection.recommendedId
                  ? t('wizard.agent.rowRecommended')
                  : t('wizard.agent.rowInstalled')
              }
              selected={choice.pickerId === selection.selectedId}
              onSelect={() => selection.select(choice.pickerId)}
            />
          ))}
        </div>
      ) : null}

      {scanning ? null : (
        <div className="mt-6 flex w-[480px] flex-col gap-3" data-testid="agent-step-advanced">
          {selection.status === 'error' ? null : (
            <>
              <button
                type="button"
                aria-expanded={showInstallGuide}
                onClick={() => setShowInstallGuide((open) => !open)}
                className={LINK_CLASS}
              >
                {t('wizard.agent.installGuide')}
              </button>
              {showInstallGuide ? (
                <div className="flex flex-col gap-1.5 pl-2">
                  {INSTALL_TARGETS.map((target) => (
                    <InstallLink key={target.url} target={target} desktop={desktop} />
                  ))}
                </div>
              ) : null}
            </>
          )}

          <button
            type="button"
            aria-expanded={showManual}
            onClick={() => setShowManual((open) => !open)}
            className={LINK_CLASS}
          >
            {t('wizard.agent.manualToggle')}
          </button>
          {showManual ? (
            <ManualCommandField
              id="onboarding-manual-launch"
              value={selection.manual}
              onChange={selection.setManual}
            />
          ) : null}
        </div>
      )}

      <div className="mt-12 flex w-[480px] items-center justify-between">
        <Button variant="tertiary" onClick={onBack}>
          {t('wizard.nav.back')}
        </Button>
        <Button variant="primary" disabled={!canContinue} onClick={advance}>
          {t('wizard.nav.next')}
        </Button>
      </div>
    </div>
  );
}
