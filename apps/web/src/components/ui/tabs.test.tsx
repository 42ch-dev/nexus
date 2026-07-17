import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { Tabs, TabsContent, TabsList, TabsTrigger } from './tabs';

/**
 * V1.121 P1 T2 — Tabs v0.4 recipe (DESIGN.md components.tabs).
 *
 * Pins hover/active recipes and the motion tokens: transitions run on
 * duration-state + ease-standard and drop under reduced motion. Selection
 * behavior (context) is unchanged.
 */
describe('Tabs (v0.4 recipes)', () => {
  function renderTabs() {
    return render(
      <Tabs value="agent" onValueChange={() => undefined}>
        <TabsList>
          <TabsTrigger value="agent">Agent</TabsTrigger>
          <TabsTrigger value="workspace">Workspace</TabsTrigger>
        </TabsList>
        <TabsContent value="agent">Agent panel</TabsContent>
        <TabsContent value="workspace">Workspace panel</TabsContent>
      </Tabs>,
    );
  }

  it('active trigger uses background-100 + shadow-card; inactive uses the hover recipe', () => {
    renderTabs();
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
    renderTabs();
    const trigger = screen.getByRole('tab', { name: 'Agent' });
    expect(trigger.className).toMatch(/\btransition-colors\b/);
    expect(trigger.className).toMatch(/\bduration-state\b/);
    expect(trigger.className).toMatch(/\bease-standard\b/);
    expect(trigger.className).toMatch(/\bmotion-reduce:transition-none\b/);
  });

  it('list consumes the background-200 well with gray-alpha-400 border', () => {
    renderTabs();
    const list = screen.getByRole('tablist');
    expect(list.className).toMatch(/\bbg-background-200\b/);
    expect(list.className).toMatch(/\bborder-gray-alpha-400\b/);
  });
});
