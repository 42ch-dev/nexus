import { useState } from 'react';

import {
  Palette,
  Users,
  Wifi,
} from 'lucide-react';

import {
  SettingsLayout,
  type SettingsLayoutSection,
} from '@42ch/nexus-ui';

const SECTIONS: readonly SettingsLayoutSection[] = [
  { id: 'appearance', label: 'Appearance', icon: Palette },
  { id: 'connection', label: 'Connection', icon: Wifi },
  { id: 'workspace', label: 'Workspace', icon: Users },
];

/**
 * Studio Components fixture for `<SettingsLayout>` (V1.150 P2 promotion).
 *
 * The promoted settings frame: section nav rail + outlet region. The caller
 * owns the section registry (labels/icons), the active selection, and the
 * navigation callback; hiddenIds is the entrance hide-table. Navigation is
 * live via the onNavigate callback.
 */
export function SettingsLayoutFixtures() {
  const [activeId, setActiveId] = useState('appearance');
  const [lastNavigated, setLastNavigated] = useState('appearance');

  return (
    <div data-testid="settings-layout-fixtures">
      <div className="border border-gray-alpha-300 rounded-card bg-background-100">
        <SettingsLayout
          sections={SECTIONS}
          activeId={activeId}
          onNavigate={(id) => {
            setActiveId(id);
            setLastNavigated(id);
          }}
          navLabel="Settings sections"
          title="Settings"
          helper="Example page chrome over the promoted settings layout."
        >
          <div data-testid="settings-layout-outlet-content" className="py-4">
            <p className="text-copy-14 text-gray-900">
              Active section: <strong>{activeId}</strong>
            </p>
            <p className="text-copy-13 text-gray-700 mt-2">
              Last navigated:{' '}
              <span data-testid="settings-layout-last-nav">{lastNavigated}</span>
            </p>
          </div>
        </SettingsLayout>
      </div>
      <p className="text-copy-13 text-gray-700 mt-4">
        Promoted{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @42ch/nexus-ui
        </code>{' '}
        SettingsLayout — section nav rail + outlet promoted from the app
        frame. The registry, active selection, and navigation stay
        caller-owned; the typed registry and URL aliasing stay app-side.
      </p>
    </div>
  );
}
