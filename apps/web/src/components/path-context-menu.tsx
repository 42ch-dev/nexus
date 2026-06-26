/**
 * `PathContextMenu` — the V1.66 desktop right-click menu for chapter paths.
 *
 * Spec: web-ui-design-requirements §6.4 + DESIGN.md "Desktop Context Menu".
 *
 * Entry order (§6.4): **Copy Path** (browser + desktop) → **Open With…**
 * (desktop only) → **Reveal in Finder** (desktop only). Browser build renders
 * Copy Path only — **no greyed-out teasing** of unavailable native actions
 * (carries V1.65 §5.3 rule forward).
 *
 * Desktop actions are surfaced only when {@link useDesktopCapabilities} returns a
 * non-null capability object (i.e. inside the Tauri webview). The authoritative
 * runtime path guard lives in the Tauri `open_with` / `reveal_in_finder` commands
 * (`apps/desktop/src-tauri/src/lib.rs`); a rejection is surfaced here as
 * plain-language copy (`Path not opened. The file is outside the active workspace.`).
 *
 * `useContextMenu` extracts the open/close + Escape/click-away handling so both
 * the body view and the outline editor share one menu implementation.
 */
import { useCallback, useEffect, useState, type ReactNode } from 'react';
import { Copy, ExternalLink, FolderSearch } from 'lucide-react';

import { useDesktopCapabilities } from '@/lib/client-context';
import { useToast } from '@/lib/use-toast';
import type { DesktopCapabilityError } from '@/lib/nexus';

export interface MenuPosition {
  x: number;
  y: number;
}

/**
 * Shared right-click menu state: position, open flag, and the click-away /
 * Escape listeners that close it. Both chapter surfaces use this so the menu
 * behavior is identical.
 */
export function useContextMenu() {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<MenuPosition>({ x: 0, y: 0 });

  const openMenu = useCallback((event: React.MouseEvent) => {
    event.preventDefault();
    setPosition({ x: event.clientX, y: event.clientY });
    setOpen(true);
  }, []);

  useEffect(() => {
    if (!open) return;
    function close() {
      setOpen(false);
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') close();
    }
    window.addEventListener('click', close, { once: true });
    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('click', close);
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [open]);

  return { open, position, openMenu, close: () => setOpen(false) } as const;
}

function isOutsideWorkspaceError(err: unknown): boolean {
  return (
    err !== null &&
    typeof err === 'object' &&
    (err as DesktopCapabilityError).code === 'path_outside_workspace'
  );
}

function errorMessage(err: unknown): string {
  if (err !== null && typeof err === 'object' && 'message' in err) {
    return String((err as { message: unknown }).message);
  }
  return err instanceof Error ? err.message : 'Action failed.';
}

export interface PathContextMenuProps {
  /** Workspace-relative or absolute path to act on (e.g. body_path / outline_path). */
  path: string;
  /** Label shown above the path preview (e.g. "Body path"). */
  pathLabel: string;
  /** Menu anchor position (clientX / clientY). */
  position: MenuPosition;
  /** Called when any item fires or the menu loses focus. */
  onClose: () => void;
  /** aria-label for the menu region (per-surface). */
  regionLabel?: string;
}

/**
 * Render the path context menu. The parent controls `position` + `onClose`
 * (typically via {@link useContextMenu}); this component owns the action
 * handlers + desktop-gated entry rendering.
 */
export function PathContextMenu({
  path,
  pathLabel,
  position,
  onClose,
  regionLabel = 'Path context menu',
}: PathContextMenuProps) {
  const desktop = useDesktopCapabilities();
  const { toast } = useToast();

  async function copyPath() {
    try {
      await navigator.clipboard.writeText(path);
      toast({ variant: 'success', title: 'Path copied' });
    } catch {
      toast({
        variant: 'error',
        title: 'Path not copied',
        description: 'Copy it manually from the details panel.',
      });
    }
    onClose();
  }

  async function runNative(
    action: 'openWith' | 'revealInFinder',
    successTitle: string,
  ): Promise<void> {
    if (!desktop) return; // defended by conditional render; belt-and-suspenders.
    try {
      await desktop[action](path);
      toast({ variant: 'success', title: successTitle });
    } catch (err) {
      // The Rust path guard returns `path_outside_workspace`; surface its
      // plain-language copy verbatim (design-req §6.4 / DESIGN.md rule).
      const title = isOutsideWorkspaceError(err)
        ? 'Path not opened'
        : action === 'openWith'
          ? 'Could not open editor'
          : 'Could not reveal in Finder';
      toast({ variant: 'error', title, description: errorMessage(err) });
    }
    onClose();
  }

  return (
    <div
      role="menu"
      aria-label={regionLabel}
      style={{ left: position.x, top: position.y }}
      className="fixed z-50 min-w-[200px] rounded-popover border border-gray-alpha-400 bg-background-100 p-1 shadow-popover"
    >
      <MenuItem onClick={() => void copyPath()} icon={<Copy className="h-4 w-4" aria-hidden />}>
        Copy Path
      </MenuItem>
      {desktop && (
        <>
          <MenuItem
            onClick={() => void runNative('openWith', 'Opened in editor')}
            icon={<ExternalLink className="h-4 w-4" aria-hidden />}
          >
            Open With…
          </MenuItem>
          <MenuItem
            onClick={() => void runNative('revealInFinder', 'Revealed in Finder')}
            icon={<FolderSearch className="h-4 w-4" aria-hidden />}
          >
            Reveal in Finder
          </MenuItem>
        </>
      )}
      {path && (
        <div className="px-3 py-2" aria-hidden>
          <span className="block max-w-[320px] truncate text-copy-13-mono text-gray-900" title={path}>
            {pathLabel}: {path}
          </span>
        </div>
      )}
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
