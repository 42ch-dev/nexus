/**
 * Workspace create panel (plan v1220 P3-T3; prototype screen 3; partial-success
 * handling added by plan v1220 QC fix wave 2).
 *
 * Opened from the chip row's trailing 「＋新建工作区」 chip on creator-entrance
 * content surfaces (世界/作品). Display name → slug via
 * {@link deriveWorkspaceSlug}; the creative root preview mirrors the core's
 * single derivation rule (`~/Nexus/<slug>`, `~/Nexus` for the `default` slug)
 * and can be overridden under the collapsed advanced section.
 *
 * 「创建并切换」 runs create → activate:
 *  - create failure keeps the draft and renders one inline `创建失败：{reason}`
 *    line — never a toast-only failure, and no chip-row change;
 *  - activation failure after a committed create is a distinct state: the panel
 *    stays open, the chip row refreshes so the created workspace is visible
 *    (not active), and 「重试激活」 re-runs ONLY set-active — a re-create would
 *    collide with the registration it already owns.
 */
import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import type { FormEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { useActiveCreatorId } from '@/lib/active-creator-context';
import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import { queryKeys } from '@/lib/nexus/query-keys';
import { deriveWorkspaceSlug } from '@/lib/workspace-slug';
import { cn } from '@/lib/utils';

/** Display-only mirror of the core's `default_creative_root` (one rule, core-owned). */
function previewRoot(slug: string): string {
  return slug === 'default' ? '~/Nexus' : `~/Nexus/${slug}`;
}

interface CreatedWorkspace {
  slug: string;
  creatorId: string;
}

export function WorkspaceCreatePanel({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation('shell');
  const client = useNexusClient();
  const qc = useQueryClient();
  const creatorId = useActiveCreatorId();
  const [displayName, setDisplayName] = useState('');
  const [creativeRoot, setCreativeRoot] = useState('');
  const [failure, setFailure] = useState<string | null>(null);
  const [created, setCreated] = useState<CreatedWorkspace | null>(null);
  const [pending, setPending] = useState(false);

  /** `创建失败：{reason}` — the typed reason per the service fold. */
  function failureReason(error: unknown): string {
    if (error instanceof NexusClientError) {
      if (error.code === 'workspace_conflict') return t('workspaceCreate.conflict');
      if (error.code === 'invalid_input') {
        const details = error.details as { reason?: string } | undefined;
        return details?.reason === 'not writable'
          ? t('workspaceCreate.notWritable')
          : t('workspaceCreate.invalid');
      }
    }
    return t('workspaceCreate.generic');
  }

  /** Activate the created workspace and refresh workspace-scoped content. */
  async function activate(workspace: CreatedWorkspace) {
    setPending(true);
    setFailure(null);
    try {
      await client.setActiveWorkspace({
        creator_id: workspace.creatorId,
        workspace_slug: workspace.slug,
      });
      void qc.invalidateQueries();
      onClose();
    } catch (error) {
      setFailure(failureReason(error));
    } finally {
      setPending(false);
    }
  }

  const trimmed = displayName.trim();
  const slug = deriveWorkspaceSlug(trimmed);
  const rootOverride = creativeRoot.trim();
  const preview = rootOverride === '' ? previewRoot(slug) : rootOverride;
  const canSubmit =
    created !== null ? !pending : trimmed !== '' && creatorId !== null && !pending;

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canSubmit) return;
    if (created !== null) {
      await activate(created);
      return;
    }

    setPending(true);
    setFailure(null);
    let response;
    try {
      response = await client.createWorkspace({
        creator_id: creatorId ?? '',
        workspace_slug: slug,
        display_name: trimmed,
        ...(rootOverride === '' ? {} : { creative_root: rootOverride }),
      });
    } catch (error) {
      setFailure(failureReason(error));
      setPending(false);
      return;
    }

    // Creation committed: the chip row must surface it even if activation fails.
    setCreated({ slug: response.workspace_slug, creatorId: response.creator_id });
    void qc.invalidateQueries({
      queryKey: queryKeys.workspaces.list({ creator_id: creatorId }),
    });
    await activate({ slug: response.workspace_slug, creatorId: response.creator_id });
  }

  return (
    <form
      data-testid="workspace-create-panel"
      onSubmit={handleSubmit}
      className="flex w-full max-w-xl flex-col gap-3 rounded-lg border border-gray-alpha-400 p-4"
    >
      <h3 className="text-label-14 font-medium text-gray-1000">{t('workspaceCreate.title')}</h3>

      <label className="flex flex-col gap-1 text-label-12 text-gray-700">
        {t('workspaceCreate.nameLabel')}
        <input
          data-testid="workspace-create-name"
          value={displayName}
          disabled={created !== null}
          onChange={(event) => setDisplayName(event.target.value)}
          placeholder={t('workspaceCreate.namePlaceholder')}
          className="rounded-md border border-gray-alpha-400 bg-transparent px-3 py-1.5 text-label-14 text-gray-1000"
        />
      </label>

      <div className="flex flex-col gap-1 text-label-12 text-gray-700">
        <span>{t('workspaceCreate.rootLabel')}</span>
        <p data-testid="workspace-root-preview" className="text-label-13 text-gray-800">
          {preview}
        </p>
      </div>

      <details className="flex flex-col gap-1 text-label-12 text-gray-700">
        <summary className="cursor-pointer">{t('workspaceCreate.advanced')}</summary>
        <label className="mt-2 flex flex-col gap-1">
          {t('workspaceCreate.rootLabelOverride')}
          <input
            data-testid="workspace-create-root"
            value={creativeRoot}
            disabled={created !== null}
            onChange={(event) => setCreativeRoot(event.target.value)}
            placeholder={t('workspaceCreate.rootPlaceholder')}
            className="rounded-md border border-gray-alpha-400 bg-transparent px-3 py-1.5 text-label-14 text-gray-1000"
          />
        </label>
        <p className="text-gray-600">{t('workspaceCreate.advancedHint')}</p>
      </details>

      {created !== null ? (
        <p data-testid="workspace-create-created" className="text-label-12 text-gray-700">
          {t('workspaceCreate.createdNotActivated')}
        </p>
      ) : null}

      {failure !== null ? (
        <p role="alert" data-testid="workspace-create-error" className="text-label-12 text-red-800">
          {created !== null
            ? t('workspaceCreate.activationFailurePrefix')
            : t('workspaceCreate.failurePrefix')}
          {failure}
        </p>
      ) : null}

      <div className="flex items-center gap-2">
        <button
          type="submit"
          data-testid="workspace-create-submit"
          disabled={!canSubmit}
          className={cn(
            'rounded-full border border-gray-1000 bg-gray-1000 px-3 py-1 text-label-12 font-medium text-background-100 transition-colors duration-state ease-standard motion-reduce:transition-none',
            !canSubmit && 'cursor-not-allowed opacity-50',
          )}
        >
          {pending
            ? t('workspaceCreate.creating')
            : created !== null
              ? t('workspaceCreate.retryActivate')
              : t('workspaceCreate.create')}
        </button>
        <button
          type="button"
          onClick={onClose}
          className="rounded-full border border-gray-alpha-400 px-3 py-1 text-label-12 font-medium text-gray-700 transition-colors duration-state ease-standard motion-reduce:transition-none hover:border-gray-1000 hover:text-gray-1000"
        >
          {t('workspaceCreate.cancel')}
        </button>
      </div>
    </form>
  );
}
