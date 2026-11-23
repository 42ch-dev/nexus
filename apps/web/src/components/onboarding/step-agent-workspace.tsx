/**
 * prototype `onboarding-v3.html` S2D).
 *
 * One merged step: agent rows (ACP pill allowed on this surface), an optional
 * custom launch command and the workspace root. The browser never resolves
 * host paths, so the root is display-only: the field shows the `creative_root`
 * the service reports for the active workspace (`Advanced: Change location` reveals it);
 * only the desktop build can pick a directory (`pickDirectory` → the desktop
 * config). The override itself lands with the deferred create route (I-000063).
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  AgentRow,
  ManualCommandField,
  ScanErrorNotice,
  useAgentSelection,
} from '@/components/onboarding/agent-selection';
import { Button, Input, Label } from '@/components/ui';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useToast } from '@/lib/use-toast';
import { useActiveWorkspaceQuery } from '@/lib/workspace-context';

const LINK_CLASS =
  'self-start text-label-14 font-medium text-brand-deep-blue transition-colors duration-state ease-standard hover:opacity-80 dark:text-blue-700 dark:hover:opacity-100';

interface StepAgentWorkspaceProps {
  onBack: () => void;
  onFinish: () => void;
}

export function StepAgentWorkspace({ onBack, onFinish }: StepAgentWorkspaceProps) {
  const { t } = useTranslation('setup');
  const { toast } = useToast();
  const desktop = useDesktopCapabilities();
  const selection = useAgentSelection();
  const activeWorkspace = useActiveWorkspaceQuery();

  const [pickedRoot, setPickedRoot] = useState<string | null>(null);
  const [showAdvanced, setShowAdvanced] = useState(false);

  const workspaceRoot = pickedRoot ?? activeWorkspace.data?.creative_root ?? null;
  const canContinue = selection.status !== 'loading' && selection.canContinue;
  const customLaunchRequired = selection.status !== 'loading' && selection.choices.length === 0;

  async function browse() {
    if (!desktop) return;
    try {
      const picked = await desktop.pickDirectory(workspaceRoot ?? '');
      if (picked === null) return;
      await desktop.setWorkspacePath(picked);
      setPickedRoot(picked);
    } catch (error) {
      toast({
        variant: 'error',
        title: t('toast.workspacePath'),
        description: errorMessage(error) || t('error.workspacePathFailed'),
      });
    }
  }

  async function finish() {
    // so the user can retry (commit() surfaces the error toast).
    if (await selection.commit()) onFinish();
  }

  return (
    <div className="flex flex-col items-center">
      <div className="text-label-12 text-gray-700">{t('wizard.developer.stepLabel')}</div>
      <h1 className="mt-4 font-display text-display-32 text-gray-1000">{t('wizard.developer.title')}</h1>
      <p className="mt-3 text-copy-14 text-gray-900">{t('wizard.developer.description')}</p>

      {selection.status === 'loading' ? (
        <p role="status" className="mt-8 text-copy-14 text-gray-900">
          {t('wizard.scan.loading')}
        </p>
      ) : null}

      {selection.status === 'error' ? (
        <div className="mt-8">
          <ScanErrorNotice reason={selection.errorReason} onRetry={selection.retry} />
        </div>
      ) : null}

      {selection.status === 'empty' ? (
        <div className="mt-8 flex flex-col items-center gap-1 text-center">
          <p className="text-heading-16 font-heading text-gray-1000">
            {t('agentPicker.empty.title')}
          </p>
          <p className="text-copy-14 text-gray-900">{t('agentPicker.empty.description')}</p>
        </div>
      ) : null}

      {selection.status === 'ready' ? (
        <div
          role="radiogroup"
          aria-label={t('wizard.developer.agentsLabel')}
          className="mt-8 flex w-[480px] flex-col gap-3"
        >
          {selection.choices.map((choice) => (
            <AgentRow
              key={choice.pickerId}
              choice={choice}
              protocol="ACP"
              description={
                choice.pickerId === selection.recommendedId
                  ? t('wizard.developer.rowRecommended')
                  : t('wizard.developer.rowInstalled')
              }
              selected={choice.pickerId === selection.selectedId}
              onSelect={() => selection.select(choice.pickerId)}
            />
          ))}
        </div>
      ) : null}

      <div className="mt-8 flex w-[480px] flex-col gap-6">
        <ManualCommandField
          id="onboarding-custom-launch"
          label={
            customLaunchRequired
              ? t('wizard.developer.customLaunch.requiredLabel')
              : t('wizard.developer.customLaunch.optionalLabel')
          }
          placeholder={t('wizard.developer.customLaunch.placeholder')}
          value={selection.manual}
          onChange={selection.setManual}
        />

        <div className="flex flex-col gap-2">
          <Label className="text-label-14 font-medium text-gray-1000">{t('wizard.developer.workspaceRoot')}</Label>
          <p
            className="rounded-control border border-gray-alpha-400 bg-background-200 px-3 py-2 text-copy-14 text-gray-900"
            data-testid="workspace-root"
          >
            {workspaceRoot ?? t('wizard.defaultWorkspace')}
          </p>

          {desktop || workspaceRoot ? (
            <button
              type="button"
              aria-expanded={showAdvanced}
              onClick={() => setShowAdvanced((open) => !open)}
              className={LINK_CLASS}
            >
              {t('wizard.changeLocation')}
            </button>
          ) : null}

          {showAdvanced ? (
            <div className="flex items-center gap-2">
              <Input
                id="onboarding-workspace-root"
                type="text"
                readOnly
                aria-label={t('wizard.developer.workspaceRoot')}
                value={workspaceRoot ?? ''}
                className="min-w-0 flex-1"
              />
              {desktop ? (
                <Button variant="secondary" onClick={() => void browse()} className="shrink-0">
                  {t('wizard.developer.browse')}
                </Button>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>

      <div className="mt-12 flex w-[480px] items-center justify-between">
        <Button variant="tertiary" onClick={onBack}>
          {t('wizard.nav.back')}
        </Button>
        <Button variant="primary" disabled={!canContinue} onClick={finish}>
          {t('wizard.developer.finish')}
        </Button>
      </div>
    </div>
  );
}
