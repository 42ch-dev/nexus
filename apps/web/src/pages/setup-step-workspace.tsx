import { useEffect, useState } from 'react';
import { ChevronLeft, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { WorkspacePathField } from '@/components/setup/workspace-path-field';
import { useDesktopCapabilities, useNexusClient } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import {
  classifySetupContinueError,
  type SetupContinueError,
} from '@/lib/setup/continue-error';
import { useToast } from '@/lib/use-toast';
import {
  lastPathSegment,
  replaceLastPathSegment,
  slugProfileSegment,
} from '@/lib/workspace-profile-slug';
import type { WizardState } from '@/pages/setup-wizard-page';

export const DEFAULT_WORKSPACE = '~/Documents/nexus/default';

interface SetupStepWorkspaceProps {
  state: WizardState;
  onChange: (state: WizardState) => void;
  onNext: () => void;
  /** Back → Agent (required after Task 3 reorder; optional while workspace still occupies first slot). */
  onBack?: () => void;
}

/**
 * Wizard Workspace step — Profile name, default path, optional Browse, bootstrap on Continue.
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
  const client = useNexusClient();
  const [loading, setLoading] = useState(true);
  const [bootstrapping, setBootstrapping] = useState(false);
  // Classified Continue-path error (AD-P0). Drives the inline `role="alert"`
  // region above the CTA row, the class-selected helper copy, and Reset
  // visibility (`showReset = class === 'migration_db'` only — never bound to
  // message presence alone, per spec product rule 3).
  const [continueError, setContinueError] = useState<SetupContinueError | null>(null);
  // Tracks which phase of the Continue path produced `continueError`, exposed
  // via `data-continue-error-phase` for testing.
  const [continueErrorPhase, setContinueErrorPhase] = useState<
    'workspace_path' | 'bootstrap' | 'display_name' | null
  >(null);
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
        if (cancelled) return;
        // AC-P1-3 (mount reconcile, AD-P1): one-time, display-only. When the
        // folder is not picked and the resolved root's last segment does not
        // already match the slug of the current display name, rewrite the
        // displayed path's last segment once. No IPC persist here —
        // `setWorkspacePath` only runs on Continue.
        const displaySlug = slugProfileSegment(state.profileDisplayName);
        const reconciled =
          !state.workspacePicked && lastPathSegment(root) !== displaySlug
            ? replaceLastPathSegment(root, displaySlug)
            : root;
        onChange({ ...state, workspaceRoot: reconciled });
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
        // QC2-C-002: clear stale error state when the author picks a new
        // workspace path so a previous failure does not persist visually.
        setContinueError(null);
        setContinueErrorPhase(null);
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
        const classified = classifySetupContinueError('workspace_path', err);
        const message = classified.message || t('error.workspacePathFailed');
        // Promote workspace-path failure from toast-only to inline error state
        // (spec: "Workspace path failures must populate continueError").
        setContinueError({ message, class: classified.class });
        setContinueErrorPhase('workspace_path');
        toast({ variant: 'error', title: t('toast.workspacePath'), description: message });
        console.error('Failed to persist workspace path:', err);
        return;
      }
    }
    setBootstrapping(true);
    setContinueError(null);
    setContinueErrorPhase(null);
    try {
      const result = await desktop.ensureSetupBootstrap();
      if (!result.already_bootstrapped) {
        toast({
          variant: 'info',
          title: t('toast.workspacePrepared'),
          description: t('toast.workspacePreparedDescription', { creatorId: result.creator_id }),
        });
      }

      const displayName = state.profileDisplayName.trim();
      if (displayName) {
        try {
          await client.updateCreator(result.creator_id, { display_name: displayName });
        } catch (err) {
          const classified = classifySetupContinueError('display_name', err);
          const message = classified.message || t('error.profileDisplayNameFailed');
          setContinueError({ message, class: classified.class });
          setContinueErrorPhase('display_name');
          toast({
            variant: 'error',
            title: t('toast.profileDisplayName'),
            description: message,
          });
          console.error('Failed to persist profile display name:', err);
          return;
        }
      }

      setContinueError(null);
      setContinueErrorPhase(null);
      onNext();
    } catch (err) {
      const classified = classifySetupContinueError('bootstrap', err);
      const message = classified.message || t('error.workspaceBootstrapFailed');
      setContinueError({ message, class: classified.class });
      setContinueErrorPhase('bootstrap');
      toast({
        variant: 'error',
        title: t('toast.workspaceBootstrapFailed'),
        // QC2-W-004: mirror the inline alert's class-selected helper instead
        // of the legacy conflated description (spec: "when toast is shown,
        // mirror the same class-selected helper").
        description:
          classified.class === 'migration_db'
            ? t('continueError.helper.migrationDb')
            : t('continueError.helper.soft'),
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

  // Reset is allowed ONLY for migration/DB-class failures (spec product rule 3,
  // AD-P0). Never bind Reset to message presence alone.
  const showReset = continueError?.class === 'migration_db';

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto" data-testid="wizard-step-body">
        <div className="my-auto flex flex-col gap-4">
          <div className="flex flex-col gap-2">
            <h2 className="font-display text-display-24 text-gray-1000">{t('step.workspace.title')}</h2>
            <p className="text-copy-14 text-gray-900">
              {t('step.workspace.description')}
            </p>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="wizard-profile-name" className="text-label-14 font-medium text-gray-1000">
              {t('profile.name.label')}
            </Label>
            <Input
              id="wizard-profile-name"
              type="text"
              value={state.profileDisplayName}
              onChange={(e) => {
                // QC2-C-002: clear stale error state when the author edits the
                // field so a previous failure does not persist visually.
                setContinueError(null);
                setContinueErrorPhase(null);
                const nextName = e.target.value;
                // AC-P1-3 / AC-P1-4: while the folder is not picked, typing a
                // Profile name updates the path's last segment to the slug of
                // the new name. Once the author picks a folder
                // (`workspacePicked: true`), name edits leave the path frozen.
                const nextRoot =
                  !state.workspacePicked && state.workspaceRoot
                    ? replaceLastPathSegment(
                        state.workspaceRoot,
                        slugProfileSegment(nextName),
                      )
                    : state.workspaceRoot;
                onChange({
                  ...state,
                  profileDisplayName: nextName,
                  workspaceRoot: nextRoot,
                });
              }}
              placeholder={t('profile.name.placeholder')}
              disabled={loading || bootstrapping || resetBusy}
              required
              aria-required="true"
              // AC-P1-2: leave scroll space above the focused input so the
              // auto scroll-into-view on focus does not cover the Workspace
              // folder label at 480px card width.
              className="scroll-mt-4"
              data-testid="wizard-profile-name"
            />
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

      {continueError ? (
        <div
          role="alert"
          data-testid="wizard-continue-error"
          data-continue-error-class={continueError.class}
          className="flex shrink-0 flex-col gap-2 rounded-card border border-error-surface-border bg-error-surface p-3"
        >
          <p className="text-heading-16 font-heading text-red-1000">{continueError.message}</p>
          <p className="text-copy-14 text-red-900">
            {continueError.class === 'migration_db'
              ? t('continueError.helper.migrationDb')
              : t('continueError.helper.soft')}
          </p>
        </div>
      ) : null}

      <div
        className="mt-auto flex shrink-0 items-center gap-setup-wizard-surface-cta-container-gap"
        data-testid="wizard-cta-row"
        data-layout="horizontal-adjacent"
        data-continue-error-phase={continueErrorPhase ?? undefined}
      >
        {onBack && (
          <Button variant="tertiary" onClick={onBack} aria-label={t('action.back')} className="px-2">
            <ChevronLeft className="h-4 w-4" aria-hidden="true" />
          </Button>
        )}
        <Button
          variant="primary"
          onClick={continueToNext}
          disabled={loading || bootstrapping || resetBusy || !state.workspaceRoot || !state.profileDisplayName.trim()}
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
        {showReset ? (
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
