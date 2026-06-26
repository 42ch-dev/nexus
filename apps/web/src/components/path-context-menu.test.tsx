/**
 * PathContextMenu conditional-rendering tests (web-ui-design-requirements §6.4).
 *
 * §6.4 rules pinned here:
 *   - Browser mode renders **Copy Path only** — no greyed-out "Open With…"
 *     teasing of an unavailable action (carries V1.65 §5.3 forward).
 *   - Desktop mode renders Copy Path → Open With… → Reveal in Finder (that order).
 *   - A path-guard rejection surfaces the plain-language message from the Rust
 *     command (`Path not opened. The file is outside the active workspace.`).
 */
import { describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { PathContextMenu } from '@/components/path-context-menu';
import { renderInApp } from '@/test/test-providers';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

const POS = { x: 10, y: 10 };

function makeDesktop(impl: Partial<DesktopCapabilities> = {}): DesktopCapabilities {
  return {
    openWith: vi.fn().mockResolvedValue(undefined),
    revealInFinder: vi.fn().mockResolvedValue(undefined),
    getDaemonStatus: vi.fn().mockResolvedValue({ state: 'running' }),
    onDaemonStatusChanged: vi.fn().mockResolvedValue(() => {}),
    startDaemon: vi.fn().mockResolvedValue(undefined),
    stopDaemon: vi.fn().mockResolvedValue(undefined),
    ...impl,
  };
}

describe('PathContextMenu conditional rendering (§6.4)', () => {
  it('browser mode renders Copy Path only (no native-action teasing)', () => {
    // desktop omitted → useDesktopCapabilities() returns null → browser build.
    renderInApp(
      <PathContextMenu path="Works/WRK/Stories/ch01.md" pathLabel="Body" position={POS} onClose={() => {}} />,
    );
    expect(screen.getByRole('menuitem', { name: /Copy Path/i })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: /Open With/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: /Reveal in Finder/i })).not.toBeInTheDocument();
  });

  it('desktop mode renders Copy Path, Open With…, and Reveal in Finder in that order', () => {
    renderInApp(
      <PathContextMenu path="Works/WRK/Stories/ch01.md" pathLabel="Body" position={POS} onClose={() => {}} />,
      { desktop: makeDesktop() },
    );
    const items = screen.getAllByRole('menuitem').map((el) => el.textContent ?? '');
    expect(items).toEqual(['Copy Path', 'Open With…', 'Reveal in Finder']);
  });

  it('invokes desktop.openWith when Open With… is clicked, then closes', async () => {
    const openWith = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    renderInApp(
      <PathContextMenu path="Works/WRK/Stories/ch01.md" pathLabel="Body" position={POS} onClose={onClose} />,
      { desktop: makeDesktop({ openWith }) },
    );
    await userEvent.click(screen.getByRole('menuitem', { name: /Open With/i }));
    expect(openWith).toHaveBeenCalledWith('Works/WRK/Stories/ch01.md');
    expect(onClose).toHaveBeenCalled();
  });

  it('invokes desktop.revealInFinder when Reveal in Finder is clicked', async () => {
    const revealInFinder = vi.fn().mockResolvedValue(undefined);
    renderInApp(
      <PathContextMenu path="x/y.md" pathLabel="Outline" position={POS} onClose={() => {}} />,
      { desktop: makeDesktop({ revealInFinder }) },
    );
    await userEvent.click(screen.getByRole('menuitem', { name: /Reveal in Finder/i }));
    expect(revealInFinder).toHaveBeenCalledWith('x/y.md');
  });

  it('surfaces the plain-language path-guard rejection toast on OutsideWorkspace', async () => {
    // Mirrors the Rust PathGuardError for a path outside the workspace root.
    const openWith = vi.fn().mockRejectedValue({
      code: 'path_outside_workspace',
      message: 'Path not opened. The file is outside the active workspace.',
    });
    renderInApp(
      <PathContextMenu path="/etc/passwd" pathLabel="Body" position={POS} onClose={() => {}} />,
      { desktop: makeDesktop({ openWith }) },
    );
    await userEvent.click(screen.getByRole('menuitem', { name: /Open With/i }));
    expect(await screen.findByText('Path not opened')).toBeInTheDocument();
    expect(
      await screen.findByText(/The file is outside the active workspace/i),
    ).toBeInTheDocument();
  });

  it('Copy Path writes the path to the clipboard in both modes', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    renderInApp(
      <PathContextMenu path="Works/WRK/Stories/ch01.md" pathLabel="Body" position={POS} onClose={() => {}} />,
    );
    await userEvent.click(screen.getByRole('menuitem', { name: /Copy Path/i }));
    expect(writeText).toHaveBeenCalledWith('Works/WRK/Stories/ch01.md');
  });
});
