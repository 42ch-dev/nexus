import { Globe, Plus, type LucideIcon } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export type CreatorEntityRef =
  | { kind: 'work'; id: string; label: string }
  | { kind: 'world'; id: string; label: string };

export type CreatorShellCreateLabels = {
  createWorldTitle: string;
  createWorldDescription: string;
  createWorkTitle: string;
  createWorkDescription: string;
  /** Tooltip when Create World is shown but not wired (honest desktop-only path). */
  createWorldDisabledTitle?: string;
};

export type CreatorShellControllerLabels = {
  title: string;
  description: string;
  /** Host-interpolated selection summary (i18n-friendly). */
  selectedSummary: string;
  back: string;
};

type CreateCardButtonProps = {
  icon: LucideIcon;
  title: string;
  description: string;
  onClick?: () => void;
  disabled?: boolean;
  disabledTitle?: string;
  testId: string;
};

function CreateCardButton({
  icon: Icon,
  title,
  description,
  onClick,
  disabled = false,
  disabledTitle,
  testId,
}: CreateCardButtonProps) {
  return (
    <button
      type="button"
      onClick={disabled ? undefined : onClick}
      disabled={disabled}
      tabIndex={disabled ? -1 : undefined}
      title={disabled ? disabledTitle : undefined}
      data-testid={testId}
      className={cn(
        'flex w-full min-h-[7.5rem] flex-col items-center justify-center gap-2 rounded-card border border-dashed border-gray-alpha-400 p-6 text-center motion-reduce:transition-none',
        disabled
          ? 'opacity-60 cursor-not-allowed'
          : 'transition-colors duration-state ease-standard hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2',
      )}
    >
      <Icon className="h-8 w-8 shrink-0 text-blue-700" aria-hidden />
      <span className="font-display text-display-20 tracking-tight text-gray-1000">{title}</span>
      <span className="max-w-sm text-copy-14 text-gray-700">{description}</span>
    </button>
  );
}

export type CreatorShellContentProps =
  | {
      mode: 'create';
      canCreateWorld: boolean;
      labels: CreatorShellCreateLabels;
      onCreateWorld?: () => void;
      onCreateWork: () => void;
      'data-testid'?: string;
    }
  | {
      mode: 'controller';
      selectedEntity: CreatorEntityRef;
      labels: CreatorShellControllerLabels;
      onBack: () => void;
      'data-testid'?: string;
    };

/**
 * Creator shell content region — Create page vs Controller Panel stub (V1.128 P2).
 *
 * Presentational extract consumed by App hub routes and Design Studio shell
 * fixtures via `@web-layout/creator-shell-content`. Host owns selection
 * context, dialog orchestration, and i18n labels.
 */
export function CreatorShellContent(props: CreatorShellContentProps) {
  const testId = props['data-testid'] ?? 'creator-shell-content';

  if (props.mode === 'create') {
    const { canCreateWorld, labels, onCreateWorld, onCreateWork } = props;

    return (
      <div
        className="flex w-full max-w-lg flex-col gap-3"
        data-testid={testId}
        data-mode="create"
      >
        {canCreateWorld ? (
          <CreateCardButton
            icon={Globe}
            title={labels.createWorldTitle}
            description={labels.createWorldDescription}
            onClick={onCreateWorld}
            testId="creator-create-world"
          />
        ) : (
          <>
            <CreateCardButton
              icon={Globe}
              title={labels.createWorldTitle}
              description={labels.createWorldDescription}
              disabled
              disabledTitle={labels.createWorldDisabledTitle}
              testId="creator-create-world"
            />
            <CreateCardButton
              icon={Plus}
              title={labels.createWorkTitle}
              description={labels.createWorkDescription}
              onClick={onCreateWork}
              testId="creator-create-work"
            />
          </>
        )}
        {canCreateWorld ? (
          <CreateCardButton
            icon={Plus}
            title={labels.createWorkTitle}
            description={labels.createWorkDescription}
            onClick={onCreateWork}
            testId="creator-create-work"
          />
        ) : null}
      </div>
    );
  }

  const { labels, onBack } = props;

  return (
    <div
      className="flex w-full max-w-lg flex-col gap-6"
      data-testid={testId}
      data-mode="controller"
    >
      <div className="flex flex-col gap-2">
        <h1 className="font-display text-display-24 text-gray-1000">{labels.title}</h1>
        <p className="text-copy-14 text-gray-700">{labels.description}</p>
        <p className="text-copy-13 text-gray-700" data-testid="creator-controller-selected">
          {labels.selectedSummary}
        </p>
      </div>
      <div>
        <Button type="button" variant="primary" size="small" onClick={onBack} data-testid="creator-controller-back">
          {labels.back}
        </Button>
      </div>
    </div>
  );
}
