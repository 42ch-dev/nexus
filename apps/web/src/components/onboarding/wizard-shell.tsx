/**
 * Onboarding wizard shell (plan v1219-onboarding T1) — route owner of `/setup`.
 *
 * Reached when the derived browser first-run gate finds no initialized
 * workspace (compass D21), and always directly reachable as the re-run entry.
 * The identity choice is the entrance, which selects the rail registry
 * (`appsForEntrance`) used after the wizard finishes; it is persisted here so a
 * finish/landing in T2–T3 already sees the chosen tree.
 */
import { useState } from 'react';

import { StepIdentity } from '@/components/onboarding/step-identity';
import type { EntranceId } from '@/components/layout/entrance-registry';
import { useEntrance } from '@/lib/entrance-context';

export function WizardShell() {
  const { setEntrance } = useEntrance();
  const [choice, setChoice] = useState<EntranceId | null>(null);

  // T2 appends the agent step here; until then the identity choice is the only
  // wizard state the shell owns.
  function handleNext() {
    if (choice === null) return;
    void setEntrance(choice);
  }

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-background-200 px-6 py-16">
      <StepIdentity value={choice} onChange={setChoice} onNext={handleNext} />
    </div>
  );
}
