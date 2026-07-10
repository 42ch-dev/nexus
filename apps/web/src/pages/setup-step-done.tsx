import { CheckCircle, ChevronLeft } from 'lucide-react';

import { Button } from '@/components/ui/button';

interface SetupStepDoneProps {
  onFinish: () => void;
  onBack?: () => void;
  isFinishing?: boolean;
}

export function SetupStepDone({ onFinish, onBack, isFinishing }: SetupStepDoneProps) {
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center gap-4 text-center">
      <div className="flex min-h-0 flex-1 flex-col items-center gap-4 overflow-y-auto" data-testid="wizard-step-body">
        <CheckCircle className="h-12 w-12 text-green-800" aria-hidden />
        <div className="flex flex-col gap-2">
          <h2 className="text-heading-24 font-heading text-gray-1000">You are ready</h2>
          <p className="text-copy-14 text-gray-900">
            Nexus is set up and ready. You can change these settings later from the app menu.
          </p>
        </div>
      </div>
      <div
        className="mt-auto flex w-full shrink-0 items-center gap-setup-wizard-surface-cta-container-gap"
        data-testid="wizard-cta-row"
        data-layout="horizontal-adjacent"
      >
        {onBack && (
          <Button variant="tertiary" onClick={onBack} aria-label="Back" className="px-2">
            <ChevronLeft className="h-4 w-4" aria-hidden="true" />
          </Button>
        )}
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
