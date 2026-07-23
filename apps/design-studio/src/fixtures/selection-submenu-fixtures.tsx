import { useState, type ReactNode } from 'react';

import { SelectionSubmenu, type SelectionMenuItem } from '@web-shell/selection-submenu'; // @web-shell/selection-submenu - transitional until package promotion criteria met

const WORLD_ITEMS: SelectionMenuItem[] = [
  { id: 'timeline', label: 'Open Timeline', onSelect: () => {} },
  { id: 'kb', label: 'Open KB', onSelect: () => {} },
  { id: 'agent', label: 'Agent: Unassigned', onSelect: () => {} },
  { id: 'rename', label: 'Rename', onSelect: () => {} },
];

const WORK_ITEMS: SelectionMenuItem[] = [
  { id: 'timeline', label: 'Open Timeline', onSelect: () => {} },
  { id: 'outline', label: 'Open Outline', onSelect: () => {} },
  { id: 'agent', label: 'Agent: Unassigned', onSelect: () => {} },
  { id: 'rename', label: 'Rename', onSelect: () => {} },
];

function VariantFrame({
  label,
  description,
  testId,
  children,
}: {
  label: string;
  description: string;
  testId: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className="rounded-card border border-gray-alpha-300 bg-background-100 p-4"
      data-testid={testId}
    >
      <p className="text-label-14 font-medium text-gray-1000 mb-1">{label}</p>
      <p className="text-copy-13 text-gray-700 mb-4">{description}</p>
      {children}
    </div>
  );
}

function MockRow({
  label,
  items,
  onOpen,
  open,
  anchorEl,
  onClose,
}: {
  label: string;
  items: SelectionMenuItem[];
  onOpen: (el: HTMLElement) => void;
  open: boolean;
  anchorEl: HTMLElement | null;
  onClose: () => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <span className="truncate font-heading text-copy-14 font-semibold text-gray-1000">{label}</span>
      <button
        type="button"
        ref={(el) => {
          if (el && !anchorEl) onOpen(el);
        }}
        onClick={(e) => onOpen(e.currentTarget)}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`Open menu for ${label}`}
        className="flex h-6 w-6 shrink-0 items-center justify-center rounded-control text-gray-400 hover:bg-gray-alpha-200 hover:text-gray-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-1"
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden>
          <circle cx="8" cy="3" r="1.5" fill="currentColor" />
          <circle cx="8" cy="8" r="1.5" fill="currentColor" />
          <circle cx="8" cy="13" r="1.5" fill="currentColor" />
        </svg>
      </button>
      <SelectionSubmenu
        open={open}
        onClose={onClose}
        anchorEl={anchorEl}
        items={items}
        ariaLabel={label}
      />
    </div>
  );
}

function WorldRowFixture() {
  const [open, setOpen] = useState(true);
  const [anchorEl, setAnchorEl] = useState<HTMLElement | null>(null);

  return (
    <MockRow
      label="My Fantasy World"
      items={WORLD_ITEMS}
      onOpen={(el) => {
        setAnchorEl(el);
        setOpen(true);
      }}
      open={open}
      anchorEl={anchorEl}
      onClose={() => setOpen(false)}
    />
  );
}

function WorkRowFixture() {
  const [open, setOpen] = useState(true);
  const [anchorEl, setAnchorEl] = useState<HTMLElement | null>(null);

  return (
    <MockRow
      label="Chapter Draft — Act I"
      items={WORK_ITEMS}
      onOpen={(el) => {
        setAnchorEl(el);
        setOpen(true);
      }}
      open={open}
      anchorEl={anchorEl}
      onClose={() => setOpen(false)}
    />
  );
}

function RenameInProgressFixture() {
  return (
    <div className="flex items-center gap-2" data-testid="selection-submenu-rename">
      <span className="truncate font-heading text-copy-14 font-semibold text-gray-1000 sr-only">
        My Fantasy World
      </span>
      <input
        type="text"
        defaultValue="My Fantasy World"
        className="h-7 flex-1 rounded-control border border-blue-1000 bg-background-100 px-2 text-copy-14 text-gray-1000 outline-none ring-1 ring-blue-1000/40 dark:border-blue-700 dark:ring-blue-700/40"
        aria-label="Rename entity"
        autoFocus
      />
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded="false"
        aria-label="Open menu for My Fantasy World"
        className="flex h-6 w-6 shrink-0 items-center justify-center rounded-control text-gray-400 hover:bg-gray-alpha-200 hover:text-gray-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-1"
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden>
          <circle cx="8" cy="3" r="1.5" fill="currentColor" />
          <circle cx="8" cy="8" r="1.5" fill="currentColor" />
          <circle cx="8" cy="13" r="1.5" fill="currentColor" />
        </svg>
      </button>
    </div>
  );
}

function InlineModalHost({
  openLabel,
  children,
}: {
  openLabel: string;
  children: (api: {
    open: boolean;
    setOpen: (v: boolean) => void;
  }) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative min-h-[120px]">
      <button
        type="button"
        className="mb-3 rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
        onClick={() => setOpen(true)}
        data-testid="selection-submenu-agent-dialog-open"
      >
        {openLabel}
      </button>
      {children({ open, setOpen })}
    </div>
  );
}

function AgentDialogPanel({ onClose }: { onClose: () => void }) {
  return (
    <div
      className="absolute inset-0 z-10 flex items-center justify-center rounded-card bg-black/40 p-4"
      role="dialog"
      aria-modal="true"
      aria-label="Assign agent to My Fantasy World"
    >
      <div className="w-full max-w-md rounded-card border border-gray-alpha-400 bg-background-100 p-6 shadow-modal">
        <h3 className="font-heading text-heading-20 text-gray-1000 mb-1">
          Assign agent to My Fantasy World
        </h3>
        <p className="text-copy-14 text-gray-700 mb-4">
          Choose an agent to manage this entity.
        </p>
        <div className="flex flex-col gap-2">
          <button
            type="button"
            className="flex w-full items-center gap-3 rounded-control border border-gray-alpha-300 bg-background-100 px-4 py-3 text-left text-copy-14 text-gray-700 hover:bg-gray-alpha-100"
          >
            <span className="flex h-8 w-8 items-center justify-center rounded-full bg-blue-100 text-label-14 text-brand-deep-blue dark:text-blue-700">
              C
            </span>
            <div className="flex-1">
              <span className="font-medium text-gray-1000">Claude</span>
              <span className="ml-2 text-label-12 text-gray-500">claude-native</span>
            </div>
          </button>
          <button
            type="button"
            className="flex w-full items-center gap-3 rounded-control border border-gray-alpha-300 bg-background-100 px-4 py-3 text-left text-copy-14 text-gray-700 hover:bg-gray-alpha-100"
          >
            <span className="flex h-8 w-8 items-center justify-center rounded-full bg-green-100 text-label-14 text-green-700">
              G
            </span>
            <div className="flex-1">
              <span className="font-medium text-gray-1000">GPT-4</span>
              <span className="ml-2 text-label-12 text-gray-500">openai-gpt4</span>
            </div>
          </button>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            className="rounded-control px-4 py-2 text-label-14 text-gray-700 hover:bg-gray-alpha-100"
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="button"
            className="rounded-control bg-brand-cyan-1000 px-4 py-2 text-label-14 text-brand-white hover:bg-blue-900 dark:bg-brand-cyan dark:text-brand-deep-blue dark:hover:bg-blue-800"
            onClick={onClose}
          >
            Assign
          </button>
        </div>
      </div>
    </div>
  );
}

function AgentDialogFixture() {
  return (
    <div data-testid="selection-submenu-agent-dialog">
      <InlineModalHost openLabel="Open agent dialog">
        {({ open, setOpen }) => (
          <>
            <div className="flex items-center gap-2">
              <span className="truncate font-heading text-copy-14 font-semibold text-gray-1000">
                My Fantasy World
              </span>
              <button
                type="button"
                aria-haspopup="menu"
                aria-expanded="false"
                aria-label="Open menu for My Fantasy World"
                className="flex h-6 w-6 shrink-0 items-center justify-center rounded-control text-gray-400 hover:bg-gray-alpha-200 hover:text-gray-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-1"
              >
                <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden>
                  <circle cx="8" cy="3" r="1.5" fill="currentColor" />
                  <circle cx="8" cy="8" r="1.5" fill="currentColor" />
                  <circle cx="8" cy="13" r="1.5" fill="currentColor" />
                </svg>
              </button>
            </div>
            {open ? <AgentDialogPanel onClose={() => setOpen(false)} /> : null}
          </>
        )}
      </InlineModalHost>
    </div>
  );
}

export function SelectionSubmenuStubFixtures() {
  return (
    <div data-testid="selection-submenu-fixtures">
      <div className="grid grid-cols-1 gap-6 sm:grid-cols-2">
        <VariantFrame
          label="World row + submenu open (light)"
          description="World row with submenu showing 4 items: Open Timeline, Open KB, Agent, Rename"
          testId="selection-submenu-world-light"
        >
          <WorldRowFixture />
        </VariantFrame>

        <VariantFrame
          label="World row + submenu open (dark)"
          description="Same as light, with dark theme"
          testId="selection-submenu-world-dark"
        >
          <div className="dark">
            <WorldRowFixture />
          </div>
        </VariantFrame>

        <VariantFrame
          label="Work row + submenu open (light)"
          description="Work row with submenu showing 4 items: Open Timeline, Open Outline, Agent, Rename"
          testId="selection-submenu-work-light"
        >
          <WorkRowFixture />
        </VariantFrame>

        <VariantFrame
          label="Work row + submenu open (dark)"
          description="Same as light, with dark theme"
          testId="selection-submenu-work-dark"
        >
          <div className="dark">
            <WorkRowFixture />
          </div>
        </VariantFrame>

        <VariantFrame
          label="Rename in progress"
          description="Submenu closed; inline-edit active on row label with blue focus ring"
          testId="selection-submenu-rename-frame"
        >
          <RenameInProgressFixture />
        </VariantFrame>

        <VariantFrame
          label="Agent dialog overlay"
          description="Submenu closed; use Open to show AgentPicker dialog with entity-name title inside a scoped host"
          testId="selection-submenu-agent-dialog-frame"
        >
          <AgentDialogFixture />
        </VariantFrame>
      </div>
    </div>
  );
}