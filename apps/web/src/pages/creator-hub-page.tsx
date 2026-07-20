import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';

import { CreatorShellContent } from '@/components/layout/presentational/creator-shell-content';
import { useCreatorEntitySelection } from '@/components/layout/creator-entity-selection-context';
import { CreateWorkDialog } from '@/pages/dialogs/create-work-dialog';

/**
 * Creator hub content — Create page vs Controller stub on `/worlds` and `/works`.
 *
 * Selection SSOT is {@link useCreatorEntitySelection}; canvas routes under
 * `/works/:workId/*` and `/worlds/:worldId/*` remain orthogonal per spec.
 *
 * Create World stays disabled until a typed `createWorld` success/error contract
 * lands (V1.127+ wire). Honest Work-create remains the enabled CTA.
 */
export function CreatorHubPage() {
  const { t: tShell } = useTranslation('shell');
  const { t: tWorlds } = useTranslation('worlds');
  const { selectedEntity, clearSelectedEntity } = useCreatorEntitySelection();
  const navigate = useNavigate();
  const [createWorkOpen, setCreateWorkOpen] = useState(false);

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
        canCreateWorld={false}
        labels={{
          createWorldTitle: tWorlds('emptyCreateWorldTitle'),
          createWorldDescription: tWorlds('emptyCreateWorldDescription'),
          createWorkTitle: tWorlds('emptyCreateWorkTitle'),
          createWorkDescription: tWorlds('emptyCreateWorkDescription'),
          createWorldDisabledTitle: tWorlds('create.desktop-only'),
        }}
        onCreateWorld={() => undefined}
        onCreateWork={() => setCreateWorkOpen(true)}
        data-testid="creator-hub-create"
      />
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
