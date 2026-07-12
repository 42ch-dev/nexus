/**
 * Settings Appearance section — language control renders, switches instantly,
 * and persists the preference.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SettingsAppearanceSection } from '@/pages/settings/settings-appearance-section';
import { renderInApp } from '@/test/test-providers';

describe('SettingsAppearanceSection', () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it('renders the language field with three options', () => {
    renderInApp(<SettingsAppearanceSection />);

    expect(
      screen.getByTestId('settings-appearance-section'),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: 'Appearance', level: 3 }),
    ).toBeInTheDocument();

    const select = screen.getByTestId(
      'settings-appearance-language-select',
    ) as HTMLSelectElement;
    expect(select).toBeInTheDocument();
    expect(select.options).toHaveLength(3);
    expect(select.options[0]).toHaveValue('system');
    expect(select.options[0]).toHaveTextContent('System');
    expect(select.options[1]).toHaveValue('en');
    expect(select.options[1]).toHaveTextContent('English');
    expect(select.options[2]).toHaveValue('zh-CN');
    expect(select.options[2]).toHaveTextContent('简体中文');

    expect(
      screen.getByTestId('settings-appearance-language-helper'),
    ).toHaveTextContent(/System follows your OS/i);
  });

  it('updates visible labels instantly when language changes', async () => {
    const user = userEvent.setup();
    renderInApp(<SettingsAppearanceSection />);

    const select = screen.getByTestId(
      'settings-appearance-language-select',
    ) as HTMLSelectElement;

    await user.selectOptions(select, 'zh-CN');

    await waitFor(() => {
      expect(
        screen.getByRole('heading', { name: '外观', level: 3 }),
      ).toBeInTheDocument();
    });

    expect(screen.getByLabelText(/语言/i)).toBeInTheDocument();
    expect(select.value).toBe('zh-CN');
    expect(select.options[0]).toHaveTextContent('跟随系统');
    expect(select.options[2]).toHaveTextContent('简体中文');
    expect(
      screen.getByTestId('settings-appearance-language-helper'),
    ).toHaveTextContent(/跟随系统/i);
  });

  it('persists the selected preference to localStorage', async () => {
    const user = userEvent.setup();
    renderInApp(<SettingsAppearanceSection />);

    const select = screen.getByTestId(
      'settings-appearance-language-select',
    ) as HTMLSelectElement;
    await user.selectOptions(select, 'zh-CN');

    await waitFor(() =>
      expect(window.localStorage.getItem('nexus-web-locale')).toBe('zh-CN'),
    );
  });
});
