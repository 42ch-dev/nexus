import { CheckCircle } from 'lucide-react';

import { Button } from '@/components/ui/button';

interface SetupStepDoneProps {
  onFinish: () => void;
  isFinishing?: boolean;
}

export function SetupStepDone({ onFinish, isFinishing }: SetupStepDoneProps) {
  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <CheckCircle className="h-12 w-12 text-green-800" aria-hidden />
      <div className="flex flex-col gap-2">
        <h2 className="text-heading-24 font-heading text-gray-1000">You are ready</h2>
        <p className="text-copy-14 text-gray-900">
          Nexus is set up and the daemon is running. You can change these settings later from the app menu.
        </p>
      </div>
      <div
        className="mt-auto flex items-center gap-setup-wizard-surface-cta-container-gap"
        data-testid="wizard-cta-row"
        data-layout="horizontal-adjacent"
      >
        <Button
          variant="primary"
          onClick={onFinish}
          disabled={isFinishing}
          className="w-full max-w-setup-wizard-surface-cta-primary-max-width"
        >
          {isFinishing ? 'Finishing…' : 'Open Nexus'}
        </Button>
      </div>
    </div>
  );
}
