import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { SettingRow, SettingsSection } from './settings-section';

/**
 * V1.150 P2 — L2 pattern family (DESIGN.md components.settings-section).
 *
 * Pins caller-visible behavior: caller-owned copy (title/description, row
 * label/hint) renders, absent optional copy leaves no trace, SettingRow wires
 * htmlFor to the caller control and keeps it interactive, and the token
 * recipe matches the FormSection voice (light + dark parity).
 */
describe('SettingsSection', () => {
  it('renders caller copy and children', () => {
    render(
      <SettingsSection title="Appearance" description="Tune the workspace look.">
        <div data-testid="body">section body</div>
      </SettingsSection>,
    );

    expect(screen.getByText('Appearance')).toBeInTheDocument();
    expect(screen.getByText('Tune the workspace look.')).toBeInTheDocument();
    expect(screen.getByTestId('body')).toBeInTheDocument();
  });

  it('omits the description when not passed', () => {
    render(<SettingsSection title="Connection">body</SettingsSection>);

    expect(screen.queryByText('Tune the workspace look.')).toBeNull();
  });

  it('merges a caller className', () => {
    const { container } = render(
      <SettingsSection title="Appearance" className="appearance-section">
        body
      </SettingsSection>,
    );

    expect(container.firstElementChild!.className).toMatch(/\bappearance-section\b/);
  });
});

describe('SettingRow', () => {
  it('renders label, control, and hint with caller copy', () => {
    render(
      <SettingRow
        label="Language"
        htmlFor="language-select"
        hint="Applies to menus and dialogs."
      >
        <select id="language-select" data-testid="language-select">
          <option value="en">English</option>
        </select>
      </SettingRow>,
    );

    const label = screen.getByText('Language');
    expect(label).toBeInTheDocument();
    expect(label).toHaveAttribute('for', 'language-select');
    expect(screen.getByTestId('language-select')).toBeInTheDocument();
    expect(screen.getByText('Applies to menus and dialogs.')).toBeInTheDocument();
  });

  it('keeps the caller control interactive', () => {
    const onChange = vi.fn();
    render(
      <SettingRow label="Language" htmlFor="language-select">
        <select
          id="language-select"
          data-testid="language-select"
          onChange={onChange}
        >
          <option value="en">English</option>
          <option value="zh-CN">简体中文</option>
        </select>
      </SettingRow>,
    );

    fireEvent.change(screen.getByTestId('language-select'), {
      target: { value: 'zh-CN' },
    });
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it('omits the hint when not passed', () => {
    render(
      <SettingRow label="Language" htmlFor="language-select">
        <select id="language-select" />
      </SettingRow>,
    );

    expect(screen.queryByText('Applies to menus and dialogs.')).toBeNull();
  });
});
