import { useEffect, useState } from 'react';
import { ChevronLeft, FolderOpen, Loader2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useToast } from '@/lib/use-toast';
import type { WizardState } from '@/pages/setup-wizard-page';

const DEFAULT_WORKSPACE = '~/Documents/nexus/default';

interface SetupStepWorkspaceProps {
  state: WizardState;
  onChange: (state: WizardState) => void;
  onNext: () => void;
  /** Back → Agent (required after Task 3 reorder; optional while workspace still occupies first slot). */
  onBack?: () => void;
}

/**
 * Wizard Workspace step — default path, optional Browse, bootstrap on Continue.
 *
 * Bootstrap timing (R-V1105P0-001 / V1.105 P1): `ensureSetupBootstrap` runs only
 * when the author clicks Continue on this step (after P0 gate Ready).
 */
export function SetupStepWorkspace({
  state,
  onChange,
  onNext,
  onBack,
}: SetupStepWorkspaceProps) {
  const desktop = useDesktopCapabilities();
  const [loading, setLoading] = useState(true);
  const [bootstrapping, setBootstrapping] = useState(false);
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
    setBootstrapping(true);
    try {
      const result = await desktop.ensureSetupBootstrap();
      if (!result.already_bootstrapped) {
        toast({
          variant: 'info',
          title: 'Local workspace prepared',
          description: `Creator identity created (${result.creator_id}).`,
        });
      }
      onNext();
    } catch (err) {
      const message = errorMessage(err) || 'Could not prepare your local workspace.';
      toast({
        variant: 'error',
        title: 'Local workspace bootstrap failed',
        description: `${message} Retry Continue, or restart the app and use Reset local database on the daemon wait splash if the problem persists.`,
      });
      console.error('Bootstrap failed:', err);
    } finally {
      setBootstrapping(false);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto" data-testid="wizard-step-body">
        <div className="flex flex-col gap-2">
          <h2 className="text-heading-24 font-heading text-gray-1000">Choose a workspace</h2>
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
      </div>

      <div
        className="mt-auto flex shrink-0 items-center gap-setup-wizard-surface-cta-container-gap"
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
          onClick={continueToNext}
          disabled={loading || bootstrapping || !state.workspaceRoot}
          className="w-full max-w-setup-wizard-surface-cta-primary-max-width"
        >
          {bootstrapping ? (
            <>
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden />
              Preparing workspace…
            </>
          ) : (
            'Continue'
          )}
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
