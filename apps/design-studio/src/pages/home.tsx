import { Link } from 'react-router';

/**
 * Studio landing page.
 *
 * Introduces the Design Studio as a read-only gallery for the Nexus DESIGN
 * SSOT, brand VI, and UI primitives. Links to all five gallery sections.
 */
export function HomePage() {
  return (
    <div className="max-w-3xl mx-auto py-16 px-4">
      <h1 className="text-heading-32 mb-4">Nexus Design Studio</h1>
      <p className="text-copy-16 text-gray-700 mb-8 max-w-prose">
        A read-only visual gallery for Nexus design tokens, brand VI, UI
        primitives, voice samples, and product chrome. Every value is driven
        by the repo-root{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1.5 py-0.5 rounded">
          DESIGN.md
        </code>{' '}
        /{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1.5 py-0.5 rounded">
          DESIGN.dark.md
        </code>{' '}
        SSOT — edit there, refresh here.
      </p>
      <p
        data-testid="home-chronos-note"
        className="text-copy-14 text-gray-700 mb-8 max-w-prose"
      >
        Chronos Light / Dark: cyan is the shared signal (buttons, active chrome, focus); deep blue
        is ink structure and light-theme links. Toggle the theme control to review both shells.
      </p>
      <p className="text-copy-14 text-gray-600 mb-8">
        No daemon required. Use the navigation above to browse each section.
      </p>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
        {[
          { label: 'Tokens', path: '/tokens', desc: 'Design scales: colors, typography, spacing, radius, elevation, motion' },
          { label: 'Brand', path: '/brand', desc: 'VI: logos, mark, theme.css swatches' },
          { label: 'Components', path: '/components', desc: 'UI primitive variant/state matrix' },
          { label: 'Voice', path: '/voice', desc: 'Voice & Content rule specimens' },
          { label: 'Surfaces', path: '/surfaces', desc: 'Chrome slices by section: Setup, Shell, AgentPicker, Daemon' },
        ].map(({ label, path, desc }) => (
          <Link
            key={path}
            to={path}
            className="block p-4 rounded-lg border border-gray-alpha-200 hover:border-gray-alpha-400 transition-colors"
          >
            <h3 className="text-heading-16 font-medium mb-1">{label}</h3>
            <p className="text-copy-14 text-gray-700">{desc}</p>
          </Link>
        ))}
      </div>
    </div>
  );
}
