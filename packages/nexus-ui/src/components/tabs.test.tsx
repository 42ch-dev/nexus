import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Tabs, TabsContent, TabsList, TabsTrigger } from './tabs';

describe('Tabs', () => {
  function renderControlledTabs(onValueChange = vi.fn()) {
    return render(
      <Tabs value="agent" onValueChange={onValueChange}>
        <TabsList>
          <TabsTrigger value="agent">Agent</TabsTrigger>
          <TabsTrigger value="workspace">Workspace</TabsTrigger>
        </TabsList>
        <TabsContent value="agent">Agent panel</TabsContent>
        <TabsContent value="workspace">Workspace panel</TabsContent>
      </Tabs>,
    );
  }

  it('controlled mode: shows the active panel and switches on trigger click', () => {
    const onValueChange = vi.fn();
    renderControlledTabs(onValueChange);

    expect(screen.getByRole('tabpanel')).toHaveTextContent('Agent panel');
    expect(screen.queryByText('Workspace panel')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('tab', { name: 'Workspace' }));
    expect(onValueChange).toHaveBeenCalledWith('workspace');
  });

  it('uncontrolled mode: uses defaultValue and updates internal selection', () => {
    render(
      <Tabs defaultValue="agent">
        <TabsList>
          <TabsTrigger value="agent">Agent</TabsTrigger>
          <TabsTrigger value="workspace">Workspace</TabsTrigger>
        </TabsList>
        <TabsContent value="agent">Agent panel</TabsContent>
        <TabsContent value="workspace">Workspace panel</TabsContent>
      </Tabs>,
    );

    expect(screen.getByRole('tabpanel')).toHaveTextContent('Agent panel');
    fireEvent.click(screen.getByRole('tab', { name: 'Workspace' }));
    expect(screen.getByRole('tabpanel')).toHaveTextContent('Workspace panel');
    expect(screen.getByRole('tab', { name: 'Workspace' })).toHaveAttribute('aria-selected', 'true');
  });

  it('transitions from uncontrolled defaultValue to controlled value', () => {
    const { rerender } = render(
      <Tabs defaultValue="agent">
        <TabsList>
          <TabsTrigger value="agent">Agent</TabsTrigger>
          <TabsTrigger value="workspace">Workspace</TabsTrigger>
        </TabsList>
        <TabsContent value="agent">Agent panel</TabsContent>
        <TabsContent value="workspace">Workspace panel</TabsContent>
      </Tabs>,
    );

    rerender(
      <Tabs value="workspace" onValueChange={vi.fn()}>
        <TabsList>
          <TabsTrigger value="agent">Agent</TabsTrigger>
          <TabsTrigger value="workspace">Workspace</TabsTrigger>
        </TabsList>
        <TabsContent value="agent">Agent panel</TabsContent>
        <TabsContent value="workspace">Workspace panel</TabsContent>
      </Tabs>,
    );

    expect(screen.getByRole('tabpanel')).toHaveTextContent('Workspace panel');
    expect(screen.getByRole('tab', { name: 'Workspace' })).toHaveAttribute('aria-selected', 'true');
  });

  it('associates triggers and panels with unique per-instance ids', () => {
    render(
      <>
        <Tabs defaultValue="shared">
          <TabsList>
            <TabsTrigger value="shared">First</TabsTrigger>
          </TabsList>
          <TabsContent value="shared">First panel</TabsContent>
        </Tabs>
        <Tabs defaultValue="shared">
          <TabsList>
            <TabsTrigger value="shared">Second</TabsTrigger>
          </TabsList>
          <TabsContent value="shared">Second panel</TabsContent>
        </Tabs>
      </>,
    );

    const triggers = screen.getAllByRole('tab');
    expect(triggers).toHaveLength(2);
    const [firstTrigger, secondTrigger] = triggers;
    expect(firstTrigger.id).not.toBe(secondTrigger.id);
    expect(firstTrigger.getAttribute('aria-controls')).not.toBe(
      secondTrigger.getAttribute('aria-controls'),
    );

    const firstPanel = document.getElementById(firstTrigger.getAttribute('aria-controls')!);
    const secondPanel = document.getElementById(secondTrigger.getAttribute('aria-controls')!);
    expect(firstPanel).toHaveTextContent('First panel');
    expect(secondPanel).toHaveTextContent('Second panel');
  });

  it('roving tabindex keeps one tab stop on the selected trigger', () => {
    renderControlledTabs();
    const active = screen.getByRole('tab', { name: 'Agent' });
    const inactive = screen.getByRole('tab', { name: 'Workspace' });
    expect(active).toHaveAttribute('tabindex', '0');
    expect(inactive).toHaveAttribute('tabindex', '-1');
  });

  it('only the selected trigger exposes aria-controls to a mounted panel', () => {
    renderControlledTabs();
    const active = screen.getByRole('tab', { name: 'Agent' });
    const inactive = screen.getByRole('tab', { name: 'Workspace' });

    expect(active).toHaveAttribute('aria-controls');
    expect(document.getElementById(active.getAttribute('aria-controls')!)).toBeInTheDocument();
    expect(inactive).not.toHaveAttribute('aria-controls');
    expect(screen.queryByText('Workspace panel')).not.toBeInTheDocument();
  });

  it('does not call onValueChange when re-clicking the already selected tab', () => {
    const onValueChange = vi.fn();
    renderControlledTabs(onValueChange);

    fireEvent.click(screen.getByRole('tab', { name: 'Agent' }));
    expect(onValueChange).not.toHaveBeenCalled();
  });

  it('initializes absent defaultValue to an empty string in uncontrolled mode', () => {
    render(
      <Tabs>
        <TabsList>
          <TabsTrigger value="only">Only</TabsTrigger>
        </TabsList>
        <TabsContent value="only">Only panel</TabsContent>
      </Tabs>,
    );

    expect(screen.queryByRole('tabpanel')).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Only' })).toHaveAttribute('aria-selected', 'false');
  });

  it('ArrowRight on the tablist selects and focuses the next tab (automatic activation)', () => {
    const onValueChange = vi.fn();
    renderControlledTabs(onValueChange);
    const list = screen.getByRole('tablist');
    fireEvent.keyDown(list, { key: 'ArrowRight' });
    expect(onValueChange).toHaveBeenCalledWith('workspace');
    expect(screen.getByRole('tab', { name: 'Workspace' })).toHaveAttribute('tabindex', '0');
  });

  it('Home and End jump to the first and last tabs', () => {
    const onValueChange = vi.fn();
    renderControlledTabs(onValueChange);
    const list = screen.getByRole('tablist');
    fireEvent.keyDown(list, { key: 'End' });
    expect(onValueChange).toHaveBeenLastCalledWith('workspace');
    fireEvent.keyDown(list, { key: 'Home' });
    expect(onValueChange).toHaveBeenLastCalledWith('agent');
  });

  it('active trigger uses background-100 + shadow-card; inactive uses the hover recipe', () => {
    renderControlledTabs();
    const active = screen.getByRole('tab', { name: 'Agent' });
    const inactive = screen.getByRole('tab', { name: 'Workspace' });

    expect(active).toHaveAttribute('aria-selected', 'true');
    expect(active.className).toMatch(/\bbg-background-100\b/);
    expect(active.className).toMatch(/\btext-gray-1000\b/);
    expect(active.className).toMatch(/\bshadow-card\b/);

    expect(inactive).toHaveAttribute('aria-selected', 'false');
    expect(inactive.className).toMatch(/\btext-gray-800\b/);
    expect(inactive.className).toMatch(/\bhover:bg-gray-alpha-100\b/);
    expect(inactive.className).toMatch(/\bhover:text-gray-1000\b/);
  });

  it('list consumes the background-200 well with gray-alpha-400 border', () => {
    renderControlledTabs();
    const list = screen.getByRole('tablist');
    expect(list.className).toMatch(/\bbg-background-200\b/);
    expect(list.className).toMatch(/\bborder-gray-alpha-400\b/);
  });
});
