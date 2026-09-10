import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Tabs, TabsContent, TabsList, TabsTrigger } from './tabs';

const order = ['Agent', 'Workspace', 'History'];

function TabSet({
  items = order,
  ...props
}: Omit<Parameters<typeof Tabs>[0], 'children'> & { items?: readonly string[] }) {
  return (
    <Tabs {...props}>
      <TabsList>
        {items.map((item) => <TabsTrigger key={item} value={item}>{item}</TabsTrigger>)}
      </TabsList>
      {items.map((item) => <TabsContent key={item} value={item}>{item} panel</TabsContent>)}
    </Tabs>
  );
}

describe('Tabs', () => {
  it('preserves caller-owned selection and one callback for each pointer activation', () => {
    const onValueChange = vi.fn();
    render(<TabSet value="Agent" onValueChange={onValueChange} />);
    fireEvent.click(screen.getByRole('tab', { name: 'Agent' }));
    expect(onValueChange).toHaveBeenCalledTimes(1);
    expect(onValueChange).toHaveBeenLastCalledWith('Agent');
    fireEvent.click(screen.getByRole('tab', { name: 'Workspace' }));
    expect(onValueChange).toHaveBeenCalledTimes(2);
    expect(onValueChange).toHaveBeenLastCalledWith('Workspace');
    expect(screen.getByRole('tabpanel')).toHaveTextContent('Agent panel');
  });

  it('navigates from actual focus even when a controlled caller does not update selection', () => {
    const onValueChange = vi.fn();
    render(<TabSet value="Agent" onValueChange={onValueChange} />);
    const workspace = screen.getByRole('tab', { name: 'Workspace' });
    workspace.focus();
    fireEvent.keyDown(workspace, { key: 'ArrowRight' });
    expect(screen.getByRole('tab', { name: 'History' })).toHaveFocus();
    expect(onValueChange).toHaveBeenCalledTimes(1);
    expect(onValueChange).toHaveBeenLastCalledWith('History');
    expect(screen.getByRole('tabpanel')).toHaveTextContent('Agent panel');
  });

  it('allows keyboard entry without a default selection and automatically activates navigation targets', () => {
    render(<TabSet />);
    expect(screen.queryByRole('tabpanel')).not.toBeInTheDocument();
    const agent = screen.getByRole('tab', { name: 'Agent' });
    expect(agent.tabIndex).toBe(0);
    expect(screen.getAllByRole('tab').filter((tab) => tab.tabIndex === 0)).toEqual([agent]);
    agent.focus();
    for (const [key, target] of [
      ['ArrowRight', 'Workspace'],
      ['End', 'History'],
      ['ArrowRight', 'Agent'],
      ['ArrowLeft', 'History'],
      ['Home', 'Agent'],
    ]) {
      fireEvent.keyDown(document.activeElement!, { key });
      const selected = screen.getByRole('tab', { name: target });
      expect(selected).toHaveFocus();
      expect(selected).toHaveAttribute('aria-selected', 'true');
      expect(screen.getByRole('tabpanel')).toHaveTextContent(`${target} panel`);
      expect(screen.getAllByRole('tab').filter((tab) => tab.tabIndex === 0)).toEqual([selected]);
    }
  });

  it('reflects keyboard selection accepted by a stateful controlled caller', () => {
    function Controlled() {
      const [value, setValue] = useState('Agent');
      return <TabSet value={value} onValueChange={setValue} />;
    }
    render(<Controlled />);
    const agent = screen.getByRole('tab', { name: 'Agent' });
    agent.focus();
    fireEvent.keyDown(agent, { key: 'ArrowRight' });
    expect(screen.getByRole('tab', { name: 'Workspace' })).toHaveFocus();
    expect(screen.getByRole('tab', { name: 'Workspace' }).tabIndex).toBe(0);
    expect(screen.getByRole('tabpanel')).toHaveTextContent('Workspace panel');
  });

  it('keeps accessible panel associations distinct for same-valued instances and whitespace values', () => {
    const value = 'project alpha / 中文';
    render(
      <>
        <Tabs defaultValue={value}>
          <TabsList><TabsTrigger value={value}>First project</TabsTrigger></TabsList>
          <TabsContent value={value}>First details</TabsContent>
        </Tabs>
        <Tabs defaultValue={value}>
          <TabsList><TabsTrigger value={value}>Second project</TabsTrigger></TabsList>
          <TabsContent value={value}>Second details</TabsContent>
        </Tabs>
      </>,
    );
    const firstPanel = screen.getByRole('tabpanel', { name: 'First project' });
    const secondPanel = screen.getByRole('tabpanel', { name: 'Second project' });
    expect(firstPanel).not.toBe(secondPanel);
    expect(firstPanel.id).not.toBe(secondPanel.id);
    for (const [name, panel] of [['First project', firstPanel], ['Second project', secondPanel]] as const) {
      const trigger = screen.getByRole('tab', { name });
      expect(document.getElementById(trigger.getAttribute('aria-controls')!)).toBe(panel);
      expect(document.getElementById(panel.getAttribute('aria-labelledby')!)).toBe(trigger);
    }
  });

  it('follows current keyed DOM order and restores an entry point after removing the selected tab', async () => {
    const { rerender } = render(<TabSet defaultValue="Agent" />);
    rerender(<TabSet defaultValue="Agent" items={['Workspace', 'Agent', 'History']} />);
    const agent = screen.getByRole('tab', { name: 'Agent' });
    agent.focus();
    fireEvent.keyDown(agent, { key: 'ArrowLeft' });
    expect(screen.getByRole('tab', { name: 'Workspace' })).toHaveFocus();
    expect(screen.getByRole('tabpanel')).toHaveTextContent('Workspace panel');
    rerender(<TabSet defaultValue="Agent" items={['Agent', 'History']} />);
    await waitFor(() => expect(screen.getByRole('tab', { name: 'Agent' }).tabIndex).toBe(0));
    expect(screen.queryByRole('tabpanel')).not.toBeInTheDocument();
    screen.getByRole('tab', { name: 'Agent' }).focus();
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowRight' });
    expect(screen.getByRole('tabpanel')).toHaveTextContent('History panel');
  });

  it('unmounts inactive panel state instead of retaining hidden children', () => {
    render(
      <Tabs defaultValue="edit">
        <TabsList>
          <TabsTrigger value="edit">Edit</TabsTrigger>
          <TabsTrigger value="preview">Preview</TabsTrigger>
        </TabsList>
        <TabsContent value="edit"><input aria-label="Draft" defaultValue="Initial" /></TabsContent>
        <TabsContent value="preview">Preview contents</TabsContent>
      </Tabs>,
    );
    fireEvent.change(screen.getByRole('textbox', { name: 'Draft' }), { target: { value: 'Changed' } });
    fireEvent.click(screen.getByRole('tab', { name: 'Preview' }));
    expect(screen.queryByRole('textbox', { name: 'Draft' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    expect(screen.getByRole('textbox', { name: 'Draft' })).toHaveValue('Initial');
  });
});
