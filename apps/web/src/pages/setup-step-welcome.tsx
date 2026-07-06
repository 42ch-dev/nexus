import { useEffect, useState } from 'react';
import { FolderOpen } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useDesktopCapabilities } from '@/lib/client-context';
import type { WizardState } from '@/pages/setup-wizard-page';

interface SetupStepWelcomeProps {
  state: WizardState;
  onChange: (state: WizardState) => void;
  onNext: () => void;
}

export function SetupStepWelcome({ state, onChange, onNext }: SetupStepWelcomeProps) {
  const desktop = useDesktopCapabilities();
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!desktop) {
      onChange({ ...state, workspaceRoot: '~/Documents/nexus42/default' });
      setLoading(false);
      return;
    }
    let cancelled = false;
    desktop
      .getWorkspaceRoot()
      .then((root) => {
        if (!cancelled) onChange({ ...state, workspaceRoot: root });
      })
      .catch(() => {
        if (!cancelled) onChange({ ...state, workspaceRoot: '~/Documents/nexus42/default' });
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [desktop, onChange]);

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-2">
        <h2 className="text-heading-24 font-heading text-gray-1000">Welcome to Nexus</h2>
        <p className="text-copy-14 text-gray-900">
          Nexus needs a workspace folder for your creative projects. We will create it if it does not exist.
        </p>
      </div>

      <div className="flex items-center gap-3 rounded-card border border-gray-alpha-400 bg-background-200 p-4">
        <FolderOpen className="h-5 w-5 text-blue-700" aria-hidden />
        <div className="flex flex-col">
          <span className="text-label-12 text-gray-700">Workspace location</span>
          <span className="text-copy-14 text-gray-1000">{loading ? 'Resolving…' : state.workspaceRoot}</span>
        </div>
      </div>

      <div className="flex justify-end">
        <Button variant="primary" onClick={onNext} disabled={loading || !state.workspaceRoot}>
          Continue
        </Button>
      </div>
    </div>
  );
}
