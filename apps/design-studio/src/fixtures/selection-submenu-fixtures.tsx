import { useRef, useState } from 'react';

import { SelectionSubmenu, type SelectionMenuItem } from '@web-shell/selection-submenu'; // @web-shell/selection-submenu - transitional until package promotion criteria met

function SelectionSubmenuDemo({
  label,
  items,
}: {
  label: string;
  items: SelectionMenuItem[];
}) {
  const [open, setOpen] = useState(false);
  const btnRef = useRef<HTMLButtonElement>(null);

  return (
    <div className="flex flex-col gap-2">
      <p className="text-label-14 text-gray-900 mb-1">{label}</p>
      <button
        ref={btnRef}
        type="button"
        onClick={() => setOpen(true)}
        className="self-start rounded-control bg-gray-alpha-100 px-3 py-1.5 text-label-14 text-gray-700 hover:bg-gray-alpha-200 hover:text-gray-1000"
      >
        Open menu
      </button>
      <SelectionSubmenu
        open={open}
        onClose={() => setOpen(false)}
        anchorEl={btnRef.current}
        items={items}
        ariaLabel={label}
      />
    </div>
  );
}

export function SelectionSubmenuStubFixtures() {
  const worldItems: SelectionMenuItem[] = [
    { id: 'timeline', label: 'Open Timeline', onSelect: () => {} },
    { id: 'kb', label: 'Open KB', onSelect: () => {} },
  ];

  const workItems: SelectionMenuItem[] = [
    { id: 'timeline', label: 'Open Timeline', onSelect: () => {} },
    { id: 'outline', label: 'Open Outline', onSelect: () => {} },
  ];

  return (
    <div
      className="flex flex-col gap-6"
      data-testid="selection-submenu-stub-fixtures"
    >
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
        <div className="rounded-card border border-gray-alpha-300 bg-background-100 p-4">
          <SelectionSubmenuDemo label="World" items={worldItems} />
        </div>
        <div className="rounded-card border border-gray-alpha-300 bg-background-100 p-4">
          <SelectionSubmenuDemo label="Work" items={workItems} />
        </div>
      </div>
    </div>
  );
}