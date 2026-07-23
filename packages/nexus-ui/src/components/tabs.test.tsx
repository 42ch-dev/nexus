import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Tabs, TabsContent, TabsList, TabsTrigger } from './tabs';

/**
 * V1.137 P2 — Tabs promotion (DESIGN.md components.tabs v0.4 recipe).
 *
 * Pins controlled/uncontrolled selection, hover/active recipes, and motion tokens.
 */
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

  it('trigger motion is token-driven (duration-state + ease-standard) and reduced-motion safe', () => {
    renderControlledTabs();
    const trigger = screen.getByRole('tab', { name: 'Agent' });
    expect(trigger.className).toMatch(/\btransition-colors\b/);
    expect(trigger.className).toMatch(/\bduration-state\b/);
    expect(trigger.className).toMatch(/\bease-standard\b/);
    expect(trigger.className).toMatch(/\bmotion-reduce:transition-none\b/);
  });

  it('list consumes the background-200 well with gray-alpha-400 border', () => {
    renderControlledTabs();
    const list = screen.getByRole('tablist');
    expect(list.className).toMatch(/\bbg-background-200\b/);
    expect(list.className).toMatch(/\bborder-gray-alpha-400\b/);
  });
});
