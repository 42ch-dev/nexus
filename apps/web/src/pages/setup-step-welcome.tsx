import { useEffect, useState } from 'react';
import { FolderOpen } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useToast } from '@/lib/use-toast';
import type { WizardState } from '@/pages/setup-wizard-page';

const DEFAULT_WORKSPACE = '~/Documents/nexus/default';

interface SetupStepWelcomeProps {
  state: WizardState;
  onChange: (state: WizardState) => void;
  onNext: () => void;
}

export function SetupStepWelcome({ state, onChange, onNext }: SetupStepWelcomeProps) {
  const desktop = useDesktopCapabilities();
  const [loading, setLoading] = useState(true);
  const { toast } = useToast();

  useEffect(() => {
    if (!desktop) {
      onChange({ ...state, workspaceRoot: DEFAULT_WORKSPACE });
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
        if (!cancelled) onChange({ ...state, workspaceRoot: DEFAULT_WORKSPACE });
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [desktop, onChange]);

  async function browse() {
    if (!desktop) return;
    setLoading(true);
    try {
      const selected = await desktop.pickDirectory(state.workspaceRoot || DEFAULT_WORKSPACE);
      if (selected) {
        onChange({ ...state, workspaceRoot: selected, workspacePicked: true });
      }
    } catch (err) {
      const message = errorMessage(err) || 'Could not open the folder picker.';
      toast({ variant: 'error', title: 'Folder picker', description: message });
      console.error('Failed to pick directory:', err);
    } finally {
      setLoading(false);
    }
  }

  async function continueToNext() {
    if (!desktop) {
      onNext();
      return;
    }
    if (shouldPersistWorkspacePath(state.workspaceRoot, state.workspacePicked)) {
      try {
        await desktop.setWorkspacePath(state.workspaceRoot);
      } catch (err) {
        const message = errorMessage(err) || 'Could not save the workspace path.';
        toast({ variant: 'error', title: 'Workspace path', description: message });
        console.error('Failed to persist workspace path:', err);
        return;
      }
    }
    onNext();
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-2">
        <h2 className="text-heading-24 font-heading text-gray-1000">Welcome to Nexus</h2>
        <p className="text-copy-14 text-gray-900">
          Nexus needs a workspace folder for your creative projects. We will create it if it does not exist.
        </p>
      </div>

      <div
        className="flex min-h-setup-wizard-surface-input-row-min-height items-center gap-setup-wizard-surface-input-row-gap rounded-control border border-setup-wizard-surface-input-row-border bg-setup-wizard-surface-input-row-bg px-setup-wizard-surface-input-row-padding-x py-setup-wizard-surface-input-row-padding-y"
        data-testid="workspace-location-row"
      >
        <FolderOpen className="h-5 w-5 text-setup-wizard-surface-input-row-icon-color" aria-hidden />
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="text-label-12 text-setup-wizard-surface-input-row-label-color">Workspace location</span>
          <span className="text-copy-14 text-setup-wizard-surface-input-row-path-color truncate">
            {loading ? 'Resolving…' : state.workspaceRoot}
          </span>
        </div>
        {desktop && (
          <Button
            variant="secondary"
            onClick={browse}
            disabled={loading}
            className="flex-shrink-0"
          >
            Browse…
          </Button>
        )}
      </div>

      <div className="flex flex-col gap-setup-wizard-surface-cta-container-gap mt-auto">
        <Button
          variant="primary"
          onClick={continueToNext}
          disabled={loading || !state.workspaceRoot}
          className="w-full max-w-setup-wizard-surface-cta-primary-max-width"
        >
          Continue
        </Button>
      </div>
    </div>
  );
}

function shouldPersistWorkspacePath(path: string, picked?: boolean): boolean {
  if (!path) return false;
  // Always persist paths the user explicitly selected via the picker.
  if (picked) return true;
  // Overwrite known stale defaults without requiring a picker interaction.
  const isOldDefault = path.includes('Documents/nexus42/');
  const isLegacyV193 = path.includes('nexus/local/default');
  return isOldDefault || isLegacyV193;
}
