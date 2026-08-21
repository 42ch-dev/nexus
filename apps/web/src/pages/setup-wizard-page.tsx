import { useCallback, useEffect, useRef, useState } from 'react';
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
import { ENTRANCE_BY_ID, type EntranceId } from '@/components/layout/entrance-registry';
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
  const { entrance, setEntrance } = useEntrance();
  const desktop = useDesktopCapabilities();
  const { toast } = useToast();
  const { t } = useTranslation('setup');
  const [step, setStep] = useState<WizardStep>('entrance');
  const [isFinishing, setIsFinishing] = useState(false);
  // W-1 (plan QC): the wizard seeds the ENTRANCE STEP from the RESOLVED stored
  // entrance — a returning install re-offers its stored choice as the
  // pre-highlighted default (desktop-shell.md §13.10.4). `entranceTouched`
  // records an explicit user re-pick so a late desktop IPC resolution cannot
  // override a deliberate choice.
  const [entranceTouched, setEntranceTouched] = useState(false);
  const entranceTouchedRef = useRef(false);
  const [state, setState] = useState<WizardState>(() => ({
    entrance,
    workspaceRoot: '',
    selectedAgent: null,
    customLaunchCommand: '',
    // AC-P1-1 / AC-P1-5: default name `default` enables Continue on first
    // paint (desktop resolves path on mount). The i18n placeholder stays
    // example copy (`My Profile`) — it is not the field value.
    profileDisplayName: 'default',
  }));
  const stateRef = useRef(state);
  stateRef.current = state;

  // Desktop resolves the persisted entrance via IPC asynchronously: once the
  // provider's read lands, re-sync the seeded choice UNLESS the user already
  // made an explicit one (W-1). `finish()` then preserves the stored value on
  // an untouched re-run instead of overwriting it with the default.
  useEffect(() => {
    if (entranceTouched || state.entrance === entrance) return;
    setState((prev) => ({ ...prev, entrance }));
  }, [entrance, entranceTouched, state.entrance]);

  /**
   * Shared step state handler — flags an explicit entrance re-pick (W-1).
   * Stable identity (useCallback) is REQUIRED: the Workspace step's mount
   * effect keys on `onChange` and would re-fire every render otherwise
   * (the pre-fix `setState` was stable; a plain closure is not).
   */
  const handleStateChange = useCallback((next: WizardState) => {
    if (!entranceTouchedRef.current && next.entrance !== stateRef.current.entrance) {
      entranceTouchedRef.current = true;
      setEntranceTouched(true);
    }
    setState(next);
  }, []);

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
                onChange={handleStateChange}
                onNext={() => setStep('agent')}
              />
            )}
            {step === 'agent' && (
              <SetupStepAgent
                state={state}
                onChange={handleStateChange}
                onNext={() => setStep('workspace')}
                onBack={() => setStep('entrance')}
              />
            )}
            {step === 'workspace' && (
              <SetupStepWorkspace
                state={state}
                onChange={handleStateChange}
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
