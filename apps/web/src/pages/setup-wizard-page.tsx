import { useState } from 'react';
import { useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';

import {
  TopStepIndicator,
  type WizardStep,
} from '@/components/setup/top-step-indicator';
import { useSetupCompleted } from '@/lib/setup-completed-context';
import { useDesktopCapabilities } from '@/lib/client-context';
import { useEntrance } from '@/lib/entrance-context';
import { errorMessage } from '@/lib/error-message';
import { useToast } from '@/lib/use-toast';
import { DEFAULT_ENTRANCE, ENTRANCE_BY_ID, type EntranceId } from '@/components/layout/entrance-registry';
import { SetupStepEntrance } from '@/pages/setup-step-entrance';
import { SetupStepAgent } from '@/pages/setup-step-agent';
import { SetupStepWorkspace } from '@/pages/setup-step-workspace';
import { SetupStepDone } from '@/pages/setup-step-done';
import type { AgentScanEntry } from '@42ch/nexus-contracts';

/** V1.170 P1 (AR-17): Entrance → Agent → Workspace → Done. */
export interface WizardState {
  /** User-layer entrance chosen on step 1 (AR-17). */
  entrance: EntranceId;
  workspaceRoot: string;
  workspacePicked?: boolean;
  selectedAgent: AgentScanEntry | null;
  customLaunchCommand: string;
  profileDisplayName: string;
}

/**
 * First-launch setup wizard — Entrance → Agent → Workspace → Done.
 *
 * Daemon readiness is owned by P0 `DaemonLaunchGate` (not a wizard step).
 * Workspace Continue runs `ensureSetupBootstrap` (R-V1105P0-001).
 * V1.105 P2: portrait card + top horizontal Steps (no left rail).
 * V1.170 P1 (AR-17): Entrance is step 1; `finish()` persists it BEFORE
 * `markCompleted()` so the post-wizard navigation lands in the right tree.
 */
export function SetupWizardPage() {
  const navigate = useNavigate();
  const { markCompleted } = useSetupCompleted();
  const { setEntrance } = useEntrance();
  const desktop = useDesktopCapabilities();
  const { toast } = useToast();
  const { t } = useTranslation('setup');
  const [step, setStep] = useState<WizardStep>('entrance');
  const [isFinishing, setIsFinishing] = useState(false);
  const [state, setState] = useState<WizardState>({
    entrance: DEFAULT_ENTRANCE,
    workspaceRoot: '',
    selectedAgent: null,
    customLaunchCommand: '',
    // AC-P1-1 / AC-P1-5: default name `default` enables Continue on first
    // paint (desktop resolves path on mount). The i18n placeholder stays
    // example copy (`My Profile`) — it is not the field value.
    profileDisplayName: 'default',
  });

  async function finish() {
    setIsFinishing(true);
    try {
      // AR-17: persist the entrance BEFORE setup completes so the first gated
      // render after `markCompleted()` already sees the chosen layout.
      await setEntrance(state.entrance);
      if (desktop) {
        const name = state.selectedAgent?.name ?? 'custom';
        const launchCommand =
          (state.selectedAgent?.launch_command ?? state.customLaunchCommand.trim()) || undefined;
        await desktop.setAgentProfile(name, launchCommand);
      }
      markCompleted();
      // Land in the chosen layout tree (AR-17): content-creator → /works,
      // developer → /developer. `landRoute` is the single source.
      navigate(ENTRANCE_BY_ID[state.entrance].landRoute, { replace: true });
    } catch (err) {
      const description = errorMessage(err) || t('error.finishSetupFailed');
      toast({ variant: 'error', title: t('toast.finishFailed'), description });
    } finally {
      setIsFinishing(false);
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-background-200 p-6">
      <div
        className="flex h-setup-wizard-wizard-max-height max-h-[85vh] w-full max-w-setup-wizard-step-wizard-max-width flex-col overflow-hidden rounded-popover border border-setup-wizard-surface-card-border bg-setup-wizard-surface-card-bg shadow-modal"
        data-testid="setup-wizard-card"
        data-shell="portrait"
      >
        <div className="flex min-h-0 flex-1 flex-col gap-4 px-setup-wizard-surface-content-panel-padding-x py-setup-wizard-surface-content-panel-padding-y">
          <TopStepIndicator currentStep={step} />
          <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
            {step === 'entrance' && (
              <SetupStepEntrance
                state={state}
                onChange={setState}
                onNext={() => setStep('agent')}
              />
            )}
            {step === 'agent' && (
              <SetupStepAgent
                state={state}
                onChange={setState}
                onNext={() => setStep('workspace')}
                onBack={() => setStep('entrance')}
              />
            )}
            {step === 'workspace' && (
              <SetupStepWorkspace
                state={state}
                onChange={setState}
                onNext={() => setStep('done')}
                onBack={() => setStep('agent')}
              />
            )}
            {step === 'done' && (
              <SetupStepDone
                onFinish={finish}
                onBack={() => setStep('workspace')}
                isFinishing={isFinishing}
              />
            )}
          </main>
        </div>
      </div>
    </div>
  );
}
