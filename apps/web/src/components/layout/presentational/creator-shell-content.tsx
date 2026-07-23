import { useState, type FormEvent } from 'react';
import { Globe, Plus, type LucideIcon } from 'lucide-react';

import { Input, Label, Select, Textarea } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

import { HubTabBar, type HubTab, type HubTabBarLabels } from './hub-tab-bar';

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

export type CreatorShellInlineCreateLabels = {
  tabs: HubTabBarLabels;
  tabsAriaLabel: string;
  world: {
    titleLabel: string;
    titlePlaceholder: string;
    submit: string;
    disabledTitle?: string;
  };
  work: {
    titleLabel: string;
    titlePlaceholder: string;
    goalLabel: string;
    goalPlaceholder: string;
    ideaLabel: string;
    ideaPlaceholder: string;
    profileLabel: string;
    profileOptions: ReadonlyArray<{ value: string; label: string }>;
    submit: string;
  };
};

export type CreatorShellInlineWorkSubmit = {
  title: string;
  longTermGoal: string;
  initialIdea: string;
  workProfile?: string;
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
      <Icon className="h-8 w-8 shrink-0 text-brand-deep-blue dark:text-blue-700" aria-hidden />
      <span className="font-display text-display-20 tracking-tight text-gray-1000">{title}</span>
      <span className="max-w-sm text-copy-14 text-gray-700">{description}</span>
    </button>
  );
}

function InlineWorldForm({
  labels,
  canCreateWorld,
  onSubmit,
}: {
  labels: CreatorShellInlineCreateLabels['world'];
  canCreateWorld: boolean;
  onSubmit?: (title: string) => void;
}) {
  const [title, setTitle] = useState('');
  const valid = title.trim().length > 0;

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (!canCreateWorld || !valid) return;
    onSubmit?.(title.trim());
    setTitle('');
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="flex flex-col gap-3"
      data-testid="sidebar-create-form-world"
      title={!canCreateWorld ? labels.disabledTitle : undefined}
    >
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="sidebar-create-world-title">{labels.titleLabel}</Label>
        <Input
          id="sidebar-create-world-title"
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          placeholder={labels.titlePlaceholder}
          disabled={!canCreateWorld}
        />
      </div>
      <Button
        type="submit"
        variant="primary"
        size="small"
        disabled={!canCreateWorld || !valid}
        data-testid="sidebar-create-submit-world"
      >
        {labels.submit}
      </Button>
    </form>
  );
}

function InlineWorkForm({
  labels,
  onSubmit,
}: {
  labels: CreatorShellInlineCreateLabels['work'];
  onSubmit?: (payload: CreatorShellInlineWorkSubmit) => void;
}) {
  const [title, setTitle] = useState('');
  const [longTermGoal, setLongTermGoal] = useState('');
  const [initialIdea, setInitialIdea] = useState('');
  const defaultProfile = labels.profileOptions[0]?.value ?? '';
  const [workProfile, setWorkProfile] = useState(defaultProfile);
  const [workProfileTouched, setWorkProfileTouched] = useState(false);

  const valid =
    title.trim().length > 0 &&
    longTermGoal.trim().length > 0 &&
    initialIdea.trim().length > 0;

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (!valid) return;
    onSubmit?.({
      title: title.trim(),
      longTermGoal: longTermGoal.trim(),
      initialIdea: initialIdea.trim(),
      ...(workProfileTouched ? { workProfile } : {}),
    });
    setTitle('');
    setLongTermGoal('');
    setInitialIdea('');
    setWorkProfile(defaultProfile);
    setWorkProfileTouched(false);
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="flex flex-col gap-3"
      data-testid="sidebar-create-form-work"
    >
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="sidebar-create-work-title">{labels.titleLabel}</Label>
        <Input
          id="sidebar-create-work-title"
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          placeholder={labels.titlePlaceholder}
        />
      </div>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="sidebar-create-work-goal">{labels.goalLabel}</Label>
        <Textarea
          id="sidebar-create-work-goal"
          value={longTermGoal}
          onChange={(event) => setLongTermGoal(event.target.value)}
          placeholder={labels.goalPlaceholder}
        />
      </div>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="sidebar-create-work-idea">{labels.ideaLabel}</Label>
        <Textarea
          id="sidebar-create-work-idea"
          value={initialIdea}
          onChange={(event) => setInitialIdea(event.target.value)}
          placeholder={labels.ideaPlaceholder}
        />
      </div>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="sidebar-create-work-profile">{labels.profileLabel}</Label>
        <Select
          id="sidebar-create-work-profile"
          value={workProfile}
          onChange={(event) => {
            setWorkProfile(event.target.value);
            setWorkProfileTouched(true);
          }}
        >
          {labels.profileOptions.map((profile) => (
            <option key={profile.value} value={profile.value}>
              {profile.label}
            </option>
          ))}
        </Select>
      </div>
      <Button
        type="submit"
        variant="primary"
        size="small"
        disabled={!valid}
        data-testid="sidebar-create-submit-work"
      >
        {labels.submit}
      </Button>
    </form>
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
      mode: 'create-inline';
      canCreateWorld: boolean;
      labels: CreatorShellInlineCreateLabels;
      onWorldSubmit?: (title: string) => void;
      onWorkSubmit?: (payload: CreatorShellInlineWorkSubmit) => void;
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

  if (props.mode === 'create-inline') {
    const { canCreateWorld, labels, onWorldSubmit, onWorkSubmit } = props;
    const [createTab, setCreateTab] = useState<HubTab>('world');

    return (
      <div
        className="flex w-full flex-col gap-3"
        data-testid={testId}
        data-mode="create-inline"
      >
        <div data-testid="sidebar-create-tab-bar">
          <HubTabBar
            activeTab={createTab}
            onTabChange={setCreateTab}
            labels={labels.tabs}
            ariaLabel={labels.tabsAriaLabel}
            tabIdPrefix="sidebar-create-tab"
            tabPanelId="sidebar-create-tabpanel"
            data-testid="sidebar-create-tab"
          />
        </div>
        <div
          id="sidebar-create-tabpanel"
          role="tabpanel"
          aria-labelledby={`sidebar-create-tab-${createTab}`}
          className="px-1"
        >
          {createTab === 'world' ? (
            <InlineWorldForm
              labels={labels.world}
              canCreateWorld={canCreateWorld}
              onSubmit={onWorldSubmit}
            />
          ) : (
            <InlineWorkForm labels={labels.work} onSubmit={onWorkSubmit} />
          )}
        </div>
      </div>
    );
  }

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
