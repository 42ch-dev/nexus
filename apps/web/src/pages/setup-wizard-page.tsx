import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { useSetupCompleted } from '@/lib/setup-completed-context';
import { useDesktopCapabilities } from '@/lib/client-context';
import { useToast } from '@/lib/use-toast';
import { SetupStepWelcome } from '@/pages/setup-step-welcome';
import { SetupStepDaemon } from '@/pages/setup-step-daemon';
import { SetupStepAgent } from '@/pages/setup-step-agent';
import { SetupStepDone } from '@/pages/setup-step-done';
import type { AgentScanEntry } from '@42ch/nexus-contracts';

export type WizardStep = 'welcome' | 'daemon' | 'agent' | 'done';

export interface WizardState {
  workspaceRoot: string;
  workspacePicked?: boolean;
  selectedAgent: AgentScanEntry | null;
  customLaunchCommand: string;
}

/**
 * First-launch 4-step setup wizard.
 *
 * Steps: welcome + workspace → daemon ready → agent detection → done.
 * Finishing persists the selected agent profile (desktop only), flips
 * `setup_completed` to true, and lands the author in the main UI.
 */
export function SetupWizardPage() {
  const navigate = useNavigate();
  const { markCompleted } = useSetupCompleted();
  const desktop = useDesktopCapabilities();
  const { toast } = useToast();
  const [step, setStep] = useState<WizardStep>('welcome');
  const [isFinishing, setIsFinishing] = useState(false);
  const [state, setState] = useState<WizardState>({
    workspaceRoot: '',
    selectedAgent: null,
    customLaunchCommand: '',
  });

  async function finish() {
    setIsFinishing(true);
    try {
      if (desktop) {
        const name = state.selectedAgent?.name ?? 'custom';
        const launchCommand =
          (state.selectedAgent?.launch_command ?? state.customLaunchCommand.trim()) || undefined;
        await desktop.setAgentProfile(name, launchCommand);
      }
      markCompleted();
      navigate('/works', { replace: true });
    } catch (err) {
      const description = err instanceof Error ? err.message : 'Failed to save agent profile.';
      toast({ variant: 'error', title: 'Could not finish setup', description });
    } finally {
      setIsFinishing(false);
    }
  }

  return (
    <div className="flex min-h-screen bg-background-100 p-6">
      <div className="flex w-full gap-6">
        <aside className="w-52 flex-shrink-0">
          <StepIndicator currentStep={step} />
        </aside>
        <main className="flex-1 max-w-setup-wizard-step-wizard-max-width rounded-card border border-gray-alpha-400 bg-background-100 p-setup-wizard-step-wizard-padding shadow-modal">
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
          {step === 'done' && (
            <SetupStepDone onFinish={finish} isFinishing={isFinishing} />
          )}
        </main>
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
    <nav aria-label="Setup progress">
      <ol className="flex flex-col">
        {steps.map((s, index) => {
          const status = index < currentIndex ? 'complete' : index === currentIndex ? 'active' : 'pending';
          return (
            <li
              key={s.id}
              className="flex h-14 items-center gap-3"
              aria-current={status === 'active' ? 'step' : undefined}
            >
              <div className="flex h-full flex-col items-center">
                <span
                  className={[
                    'flex h-setup-wizard-step-circle-size w-setup-wizard-step-circle-size items-center justify-center rounded-full text-button-14 font-button transition-colors',
                    status === 'active'
                      ? 'bg-setup-wizard-step-circle-active-bg text-setup-wizard-step-circle-active-text'
                      : status === 'complete'
                        ? 'bg-setup-wizard-step-circle-complete-bg text-setup-wizard-step-circle-complete-text'
                        : 'bg-setup-wizard-step-circle-pending-bg text-setup-wizard-step-circle-pending-text',
                  ].join(' ')}
                >
                  {index + 1}
                </span>
                {index < steps.length - 1 && (
                  <div className="w-px flex-1 bg-setup-wizard-step-connector" aria-hidden />
                )}
              </div>
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
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
