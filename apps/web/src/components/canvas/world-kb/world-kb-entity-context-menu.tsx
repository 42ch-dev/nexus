/**
 * World KB entity node context menu (V1.74 A6).
 *
 * Right-click on a canvas entity opens this menu. The only browser-available
 * action in this slice is "Connect to…", which opens the relationship inspector
 * in create mode with the source entity pre-filled.
 */
import type { ReactNode } from 'react';
import { Link2 } from 'lucide-react';

export interface WorldKbEntityContextMenuProps {
  position: { x: number; y: number };
  entityName: string;
  onClose: () => void;
  onConnectTo: () => void;
}

export function WorldKbEntityContextMenu({
  position,
  entityName,
  onClose,
  onConnectTo,
}: WorldKbEntityContextMenuProps) {
  return (
    <div
      role="menu"
      aria-label={`Actions for ${entityName}`}
      style={{ left: position.x, top: position.y }}
      className="fixed z-50 min-w-[200px] rounded-popover border border-gray-alpha-400 bg-background-100 p-1 shadow-popover"
    >
      <MenuItem
        onClick={() => {
          onClose();
          onConnectTo();
        }}
        icon={<Link2 className="h-4 w-4" aria-hidden />}
      >
        Connect to…
      </MenuItem>
    </div>
  );
}

function MenuItem({
  onClick,
  icon,
  children,
}: {
  onClick: () => void;
  icon: ReactNode;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      className="flex h-9 w-full items-center gap-2 rounded-control px-3 text-copy-14 text-gray-1000 hover:bg-gray-alpha-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-700/40"
    >
      <span className="text-gray-900" aria-hidden>
        {icon}
      </span>
      {children}
    </button>
  );
}
