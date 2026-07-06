import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { useSetupCompleted } from '@/lib/setup-completed-context';
import { SetupStepWelcome } from '@/pages/setup-step-welcome';
import { SetupStepDaemon } from '@/pages/setup-step-daemon';
import { SetupStepAgent } from '@/pages/setup-step-agent';
import { SetupStepDone } from '@/pages/setup-step-done';
import type { AgentScanEntry } from '@42ch/nexus-contracts';

export type WizardStep = 'welcome' | 'daemon' | 'agent' | 'done';

export interface WizardState {
  workspaceRoot: string;
  selectedAgent: AgentScanEntry | null;
  customLaunchCommand: string;
}

/**
 * First-launch 4-step setup wizard.
 *
 * Steps: welcome + workspace → daemon ready → agent detection → done.
 * Finishing flips `setup_completed` to true and lands the author in the main UI.
 */
export function SetupWizardPage() {
  const navigate = useNavigate();
  const { markCompleted } = useSetupCompleted();
  const [step, setStep] = useState<WizardStep>('welcome');
  const [state, setState] = useState<WizardState>({
    workspaceRoot: '',
    selectedAgent: null,
    customLaunchCommand: '',
  });

  function finish() {
    markCompleted();
    navigate('/works', { replace: true });
  }

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-background-100 p-6">
      <div className="w-full max-w-setup-wizard-step-wizard-max-width rounded-card border border-gray-alpha-400 bg-background-100 p-setup-wizard-step-wizard-padding shadow-modal">
        <StepIndicator currentStep={step} />
        <div className="mt-8">
          {step === 'welcome' && (
            <SetupStepWelcome
              state={state}
              onChange={setState}
              onNext={() => setStep('daemon')}
            />
          )}
          {step === 'daemon' && (
            <SetupStepDaemon
              onNext={() => setStep('agent')}
              onBack={() => setStep('welcome')}
            />
          )}
          {step === 'agent' && (
            <SetupStepAgent
              state={state}
              onChange={setState}
              onNext={() => setStep('done')}
              onBack={() => setStep('daemon')}
            />
          )}
          {step === 'done' && <SetupStepDone onFinish={finish} />}
        </div>
      </div>
    </div>
  );
}

function StepIndicator({ currentStep }: { currentStep: WizardStep }) {
  const steps: { id: WizardStep; label: string }[] = [
    { id: 'welcome', label: 'Welcome' },
    { id: 'daemon', label: 'Daemon' },
    { id: 'agent', label: 'Agent' },
    { id: 'done', label: 'Done' },
  ];
  const currentIndex = steps.findIndex((s) => s.id === currentStep);

  return (
    <div className="flex items-center justify-between gap-2">
      {steps.map((s, index) => {
        const status = index < currentIndex ? 'complete' : index === currentIndex ? 'active' : 'pending';
        return (
          <div key={s.id} className="flex flex-1 items-center gap-2">
            <div className="flex flex-col items-center gap-1">
              <span
                className={[
                  'flex h-setup-wizard-step-circle-size w-setup-wizard-step-circle-size items-center justify-center rounded-full text-button-14 font-button transition-colors',
                  status === 'active'
                    ? 'bg-setup-wizard-step-circle-active-bg text-setup-wizard-step-circle-active-text'
                    : status === 'complete'
                      ? 'bg-setup-wizard-step-circle-complete-bg text-setup-wizard-step-circle-complete-text'
                      : 'bg-setup-wizard-step-circle-pending-bg text-setup-wizard-step-circle-pending-text',
                ].join(' ')}
                aria-current={status === 'active' ? 'step' : undefined}
              >
                {index + 1}
              </span>
              <span
                className={[
                  'text-setup-wizard-step-label-typography',
                  status === 'pending'
                    ? 'text-setup-wizard-step-label-pending-color'
                    : 'text-setup-wizard-step-label-active-color',
                ].join(' ')}
              >
                {s.label}
              </span>
            </div>
            {index < steps.length - 1 && (
              <div
                className="h-px flex-1 bg-setup-wizard-step-connector"
                aria-hidden
              />
            )}
          </div>
        );
      })}
    </div>
  );
}
