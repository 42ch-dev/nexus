/**
 * Settings Workspace section — V1.104 P0 workspace path change (W2).
 *
 * View/change the workspace folder via desktop capabilities. Browser build
 * shows honest desktop-only copy and disables the picker.
 */
import { useEffect, useState } from 'react';
import { FolderOpen } from 'lucide-react';

import { useTranslation } from 'react-i18next';

import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { WorkspacePathField } from '@/components/setup/workspace-path-field';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useToast } from '@/lib/use-toast';

export function SettingsWorkspaceSection() {
  const { t } = useTranslation('settings');
  const { t: commonT } = useTranslation('common');
  const desktop = useDesktopCapabilities();
  const { toast } = useToast();
  const [path, setPath] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (!desktop) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    desktop
      .getWorkspaceRoot()
      .then((root) => {
        if (!cancelled) setPath(root);
      })
      .catch((err) => {
        if (!cancelled) {
          const description = errorMessage(err) || commonT('toast.workspacePathLoadFailed');
          toast({ variant: 'error', title: commonT('toast.workspacePath'), description });
          console.error('Failed to load workspace root:', err);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [desktop, toast]);

  async function handleChangeFolder() {
    if (!desktop) return;
    setSaved(false);
    setSaving(true);
    try {
      const selected = await desktop.pickDirectory(path);
      if (!selected) return;
      await desktop.setWorkspacePath(selected);
      setPath(selected);
      setSaved(true);
    } catch (err) {
      const description = errorMessage(err) || commonT('toast.workspacePathSaveFailed');
      toast({ variant: 'error', title: commonT('toast.workspacePath'), description });
      console.error('Failed to change workspace path:', err);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div
      className="flex flex-col gap-6"
      data-testid="settings-workspace-section"
      data-desktop={desktop ? 'true' : 'false'}
    >
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">{t('workspace.title')}</h3>
        <p className="text-copy-14 text-gray-900">{t('workspace.helper')}</p>
      </div>

      <Card className="shadow-card" data-testid="settings-workspace-card">
        <CardHeader>
          <div className="flex items-center gap-2">
            <FolderOpen className="h-5 w-5 text-blue-700" aria-hidden="true" />
            <CardTitle>{t('workspace.folderLabel')}</CardTitle>
          </div>
        </CardHeader>
        <CardContent className="space-y-6">
          <WorkspacePathField
            id="settings-workspace-path"
            path={path}
            loading={loading}
            changeDisabled={saving}
            onChangeClick={() => void handleChangeFolder()}
            layout="settings-row"
            desktopAvailable={Boolean(desktop)}
            label={t('workspace.folderLabel')}
            changeAction={t('workspace.changeFolder')}
            browserOnlyHelper={t('workspace.browserOnly')}
            data-testid="settings-workspace-field"
          />

          {saved && (
            <div
              className="rounded-control border border-gray-alpha-400 bg-background-200 p-4 space-y-1"
              data-testid="settings-workspace-saved-honesty"
            >
              <p className="text-copy-14 text-gray-900">
                {t('workspace.savedHonesty')}
              </p>
              <p className="text-copy-13 text-gray-700">{t('workspace.restartLabel')}</p>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
