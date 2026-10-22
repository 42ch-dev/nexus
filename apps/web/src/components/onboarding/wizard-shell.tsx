/**
 * Onboarding wizard shell (plan v1219-onboarding T1/T2) — route owner of
 * `/setup`.
 *
 * Reached when the derived browser first-run gate finds no initialized
 * workspace (compass D21), and always directly reachable as the re-run entry.
 * The identity choice is the entrance: it selects the rail registry
 * (`appsForEntrance`) used after the wizard finishes and decides the second
 * step — creator: AI partner; developer: agent + workspace.
 */
import { useState } from 'react';

import { StepAgent } from '@/components/onboarding/step-agent';
import { StepAgentWorkspace } from '@/components/onboarding/step-agent-workspace';
import { StepIdentity } from '@/components/onboarding/step-identity';
import type { EntranceId } from '@/components/layout/entrance-registry';
import { useEntrance } from '@/lib/entrance-context';

type WizardStep = 'identity' | 'agent' | 'agent-workspace';

export function WizardShell() {
  const { setEntrance } = useEntrance();
  const [choice, setChoice] = useState<EntranceId | null>(null);
  const [step, setStep] = useState<WizardStep>('identity');

  function handleIdentityNext() {
    if (choice === null) return;
    // Persisted here so a finish/landing already sees the chosen rail tree.
    void setEntrance(choice);
    setStep(choice === 'developer' ? 'agent-workspace' : 'agent');
  }

  function handleAdvance() {
    // T3 continues the machine: the creator's first-World step and the finish
    // sequence (workspace selection → landing) attach here.
  }

  function backToIdentity() {
    setStep('identity');
  }

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-background-200 px-6 py-16">
      {step === 'identity' ? (
        <StepIdentity value={choice} onChange={setChoice} onNext={handleIdentityNext} />
      ) : null}
      {step === 'agent' ? <StepAgent onBack={backToIdentity} onNext={handleAdvance} /> : null}
      {step === 'agent-workspace' ? (
        <StepAgentWorkspace onBack={backToIdentity} onFinish={handleAdvance} />
      ) : null}
    </div>
  );
}
