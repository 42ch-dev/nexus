import { useTranslation } from 'react-i18next';

import { CreatorEntityListsPanel } from '@/components/layout/creator-entity-lists-panel';
import { CreatorShellContent } from '@/components/layout/presentational/creator-shell-content';
import { useCreatorEntitySelection } from '@/components/layout/creator-entity-selection-context';

/**
 * Creator hub content — right-side Worlds/Works lists vs Controller stub on
 * `/worlds` and `/works`.
 *
 * Create actions live in the left {@link Sidebar} Create-only panel (V1.132 P3).
 * Selection SSOT is {@link useCreatorEntitySelection}; canvas routes under
 * `/works/:workId/*` and `/worlds/:worldId/*` remain orthogonal per spec.
 */
export function CreatorHubPage() {
  const { t: tShell } = useTranslation('shell');
  const { selectedEntity, clearSelectedEntity } = useCreatorEntitySelection();

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

  return <CreatorEntityListsPanel />;
}
