import { useEffect, useState } from 'react';
import { ChevronLeft, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { WorkspacePathField } from '@/components/setup/workspace-path-field';
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
  const { t } = useTranslation('setup');
  const desktop = useDesktopCapabilities();
  const [loading, setLoading] = useState(true);
  const [bootstrapping, setBootstrapping] = useState(false);
  const [bootstrapError, setBootstrapError] = useState<string | null>(null);
  const [resetBusy, setResetBusy] = useState(false);
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
      const message = errorMessage(err) || t('error.folderPickerFailed');
      toast({ variant: 'error', title: t('toast.folderPicker'), description: message });
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
        const message = errorMessage(err) || t('error.workspacePathFailed');
        toast({ variant: 'error', title: t('toast.workspacePath'), description: message });
        console.error('Failed to persist workspace path:', err);
        return;
      }
    }
    setBootstrapping(true);
    setBootstrapError(null);
    try {
      const result = await desktop.ensureSetupBootstrap();
      if (!result.already_bootstrapped) {
        toast({
          variant: 'info',
          title: t('toast.workspacePrepared'),
          description: t('toast.workspacePreparedDescription', { creatorId: result.creator_id }),
        });
      }
      setBootstrapError(null);
      onNext();
    } catch (err) {
      const message = errorMessage(err) || t('error.workspaceBootstrapFailed');
      setBootstrapError(message);
      toast({
        variant: 'error',
        title: t('toast.workspaceBootstrapFailed'),
        description: t('toast.workspaceBootstrapFailedDescription', { message }),
      });
      console.error('Bootstrap failed:', err);
    } finally {
      setBootstrapping(false);
    }
  }

  async function resetLocalDatabase() {
    if (!desktop) return;
    setResetBusy(true);
    try {
      await desktop.resetLocalDatabase();
      // Explicit D2 decision: do NOT call startDaemon after reset — reload
      // re-runs `.setup()` which always starts/attaches the sidecar.
      window.location.reload();
    } catch (err) {
      setResetBusy(false);
      const message = errorMessage(err) || t('error.resetDatabaseFailed');
      toast({
        variant: 'error',
        title: t('toast.resetLocalDatabase'),
        description: message,
      });
      console.error('Failed to reset local database:', err);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto" data-testid="wizard-step-body">
        <div className="my-auto flex flex-col gap-4">
          <div className="flex flex-col gap-2">
            <h2 className="text-heading-24 font-heading text-gray-1000">{t('step.workspace.title')}</h2>
            <p className="text-copy-14 text-gray-900">
              {t('step.workspace.description')}
            </p>
          </div>

          <WorkspacePathField
            id="wizard-workspace-path"
            path={state.workspaceRoot}
            loading={loading}
            changeDisabled={loading}
            onChangeClick={browse}
            layout="wizard-stack"
            desktopAvailable={Boolean(desktop)}
            data-testid="workspace-location-row"
          />
      </div>
    </div>

      <div
        className="mt-auto flex shrink-0 items-center gap-setup-wizard-surface-cta-container-gap"
        data-testid="wizard-cta-row"
        data-layout="horizontal-adjacent"
      >
        {onBack && (
          <Button variant="tertiary" onClick={onBack} aria-label={t('action.back')} className="px-2">
            <ChevronLeft className="h-4 w-4" aria-hidden="true" />
          </Button>
        )}
        <Button
          variant="primary"
          onClick={continueToNext}
          disabled={loading || bootstrapping || resetBusy || !state.workspaceRoot}
          className="w-full max-w-setup-wizard-surface-cta-primary-max-width"
        >
          {bootstrapping ? (
            <>
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden />
              {t('action.preparingWorkspace')}
            </>
          ) : (
            t('action.continue')
          )}
        </Button>
        {bootstrapError && desktop ? (
          <Button
            variant="tertiary"
            onClick={() => void resetLocalDatabase()}
            disabled={loading || bootstrapping || resetBusy}
          >
            {resetBusy ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" aria-hidden />
                {t('action.resetting')}
              </>
            ) : (
              t('action.resetLocalDatabase')
            )}
          </Button>
        ) : null}
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
