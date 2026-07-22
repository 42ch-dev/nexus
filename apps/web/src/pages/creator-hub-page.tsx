import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';

import { CreatorShellContent } from '@/components/layout/presentational/creator-shell-content';
import { useCreatorEntitySelection } from '@/components/layout/creator-entity-selection-context';
import { CreateWorldDialog } from '@/components/worlds/create-world-dialog';
import { useNexusClient } from '@/lib/client-context';
import { hasCreateWorldClient } from '@/lib/nexus/create-world';
import { CreateWorkDialog } from '@/pages/dialogs/create-work-dialog';

/**
 * Creator hub content — Create page vs Controller stub on `/worlds` and `/works`.
 *
 * Selection SSOT is {@link useCreatorEntitySelection}; canvas routes under
 * `/works/:workId/*` and `/worlds/:worldId/*` remain orthogonal per spec.
 *
 * Create World uses the typed `POST /v1/daemon/worlds` wire path (V1.130).
 */
export function CreatorHubPage() {
  const { t: tShell } = useTranslation('shell');
  const { t: tWorlds } = useTranslation('worlds');
  const { selectedEntity, clearSelectedEntity } = useCreatorEntitySelection();
  const navigate = useNavigate();
  const client = useNexusClient();
  const canCreateWorld = useMemo(() => hasCreateWorldClient(client), [client]);
  const [createWorkOpen, setCreateWorkOpen] = useState(false);
  const [createWorldOpen, setCreateWorldOpen] = useState(false);

  if (selectedEntity) {
    const kindLabel =
      selectedEntity.kind === 'world'
        ? tShell('creator.entityKind.world')
        : tShell('creator.entityKind.work');

    return (
      <CreatorShellContent
        mode="controller"
        selectedEntity={selectedEntity}
        labels={{
          title: tShell('creator.controllerTitle'),
          description: tShell('creator.controllerDescription'),
          selectedSummary: tShell('creator.controllerSelected', {
            kind: kindLabel,
            label: selectedEntity.label,
          }),
          back: tShell('creator.controllerBack'),
        }}
        onBack={clearSelectedEntity}
        data-testid="creator-hub-controller"
      />
    );
  }

  return (
    <>
      <CreatorShellContent
        mode="create"
        canCreateWorld={canCreateWorld}
        labels={{
          createWorldTitle: tWorlds('emptyCreateWorldTitle'),
          createWorldDescription: tWorlds('emptyCreateWorldDescription'),
          createWorkTitle: tWorlds('emptyCreateWorkTitle'),
          createWorkDescription: tWorlds('emptyCreateWorkDescription'),
          createWorldDisabledTitle: tWorlds('create.desktop-only'),
        }}
        onCreateWorld={() => setCreateWorldOpen(true)}
        onCreateWork={() => setCreateWorkOpen(true)}
        data-testid="creator-hub-create"
      />
      <CreateWorldDialog open={createWorldOpen} onOpenChange={setCreateWorldOpen} />
      <CreateWorkDialog
        open={createWorkOpen}
        onOpenChange={setCreateWorkOpen}
        onCreated={(workId) => {
          navigate(`/works/${encodeURIComponent(workId)}/outline`);
        }}
      />
    </>
  );
}
