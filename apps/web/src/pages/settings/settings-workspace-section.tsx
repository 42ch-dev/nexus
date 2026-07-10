/**
 * Settings Workspace section — V1.104 P0 workspace path change (W2).
 *
 * View/change the workspace folder via desktop capabilities. Browser build
 * shows honest desktop-only copy and disables the picker.
 */
import { useEffect, useState } from 'react';
import { FolderOpen } from 'lucide-react';

import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import {
  WorkspacePathField,
  WORKSPACE_PATH_FIELD_LABEL,
} from '@/components/setup/workspace-path-field';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useToast } from '@/lib/use-toast';

/** Locked by settings-workspace-section.md — section body helper (sentence case). */
const WORKSPACE_SECTION_HELPER =
  'View or change where Nexus stores your creative files on this machine.';

const WORKSPACE_POST_PERSIST_SUCCESS =
  'Workspace path saved. Restart or reload the app so the running daemon uses the new location.';

/** Copy-only label — no wired app restart orchestration. */
const WORKSPACE_RESTART_LABEL = 'Quit and reopen Nexus';

const WORKSPACE_BROWSER_HELPER =
  'Workspace path changes are available on the desktop app only.';

export function SettingsWorkspaceSection() {
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
          const description = errorMessage(err) || 'Could not load the workspace path.';
          toast({ variant: 'error', title: 'Workspace path', description });
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
      const description = errorMessage(err) || 'Could not save the workspace path.';
      toast({ variant: 'error', title: 'Workspace path', description });
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
        <h3 className="text-heading-16 font-heading text-gray-1000">Workspace</h3>
        <p className="text-copy-14 text-gray-900">{WORKSPACE_SECTION_HELPER}</p>
      </div>

      <Card className="shadow-card" data-testid="settings-workspace-card">
        <CardHeader>
          <div className="flex items-center gap-2">
            <FolderOpen className="h-5 w-5 text-blue-700" aria-hidden="true" />
            <CardTitle>{WORKSPACE_PATH_FIELD_LABEL}</CardTitle>
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
            browserOnlyHelper={WORKSPACE_BROWSER_HELPER}
            data-testid="settings-workspace-field"
          />

          {saved && (
            <div
              className="rounded-control border border-gray-alpha-400 bg-background-200 p-4 space-y-1"
              data-testid="settings-workspace-saved-honesty"
            >
              <p className="text-copy-14 text-gray-900">
                {WORKSPACE_POST_PERSIST_SUCCESS}
              </p>
              <p className="text-copy-13 text-gray-700">{WORKSPACE_RESTART_LABEL}</p>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
