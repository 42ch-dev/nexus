import { Link } from 'react-router';

import { StudioShellLogo } from '@/components/studio-shell-logo';

const CONTRIBUTOR_JOBS = [
  {
    title: 'Tune tokens & themes',
    detail: 'Edit repo-root DESIGN.md / DESIGN.dark.md, refresh the gallery, and verify light/dark parity on semantic scales.',
  },
  {
    title: 'Review component states',
    detail: 'Use the Components matrix for variant, focus, disabled, and portal behavior before wiring product screens.',
  },
  {
    title: 'Validate brand VI',
    detail: 'Confirm frozen logo geometry, installed-asset pigment labels, theme.css swatches, and clear-space guidance.',
  },
  {
    title: 'Check voice specimens',
    detail: 'Compare labeled writing-pattern fixtures against DESIGN § Voice & Content — no marketing substitutions.',
  },
  {
    title: 'Inspect surface chrome',
    detail: 'Browse Setup, Shell, Canvas, AgentPicker, Daemon, Launch, and Selection Submenu fixtures without the daemon.',
  },
] as const;

const GALLERY_LINKS = [
  { label: 'Tokens', path: '/tokens', desc: 'Colors, typography, spacing, radius, elevation, motion' },
  { label: 'Brand', path: '/brand', desc: 'Logos, mark, theme.css swatches, clear space' },
  { label: 'Components', path: '/components', desc: 'Primitive variant/state matrix incl. Dialog & Toast' },
  { label: 'Voice', path: '/voice', desc: 'Voice & Content rule specimens' },
  { label: 'Surfaces', path: '/surfaces', desc: 'Setup, Shell, Canvas, AgentPicker, Daemon, Launch, Selection Submenu' },
] as const;

/**
 * Studio landing page — concise contributor job overview (P2 Task 1).
 */
export function HomePage() {
  return (
    <div className="mx-auto max-w-3xl px-4 py-10">
      <div className="mb-6 flex items-center gap-3">
        <StudioShellLogo />
        <h1 className="text-heading-24 font-semibold text-gray-1000">Nexus Design Studio</h1>
      </div>

      <p className="mb-4 max-w-prose text-copy-16 text-gray-700">
        Read-only gallery for the repo-root{' '}
        <code className="rounded bg-gray-alpha-100 px-1.5 py-0.5 text-copy-13-mono">DESIGN.md</code>{' '}
        /{' '}
        <code className="rounded bg-gray-alpha-100 px-1.5 py-0.5 text-copy-13-mono">
          DESIGN.dark.md
        </code>{' '}
        SSOT. Edit those files locally, refresh here — no daemon or desktop app required.
      </p>

      <p className="mb-8 text-copy-14 text-gray-700">
        Installed logo assets remain labeled frozen references; live shell colors follow the silver-neutral /
        graphite-dark token pair and cobalt interaction signal.
      </p>

      <h2 className="mb-3 text-heading-20 font-semibold text-gray-1000">Contributor jobs</h2>
      <ol className="mb-8 space-y-3">
        {CONTRIBUTOR_JOBS.map((job, index) => (
          <li key={job.title} className="rounded-card border border-gray-alpha-200 bg-background-100 p-4">
            <p className="text-heading-16 font-medium text-gray-1000">
              {index + 1}. {job.title}
            </p>
            <p className="mt-1 text-copy-14 text-gray-700">{job.detail}</p>
          </li>
        ))}
      </ol>

      <h2 className="mb-3 text-heading-20 font-semibold text-gray-1000">Galleries</h2>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {GALLERY_LINKS.map(({ label, path, desc }) => (
          <Link
            key={path}
            to={path}
            className="block rounded-lg border border-gray-alpha-200 p-4 no-underline transition-colors hover:border-gray-alpha-400"
          >
            <h3 className="mb-1 text-heading-16 font-medium text-gray-1000">{label}</h3>
            <p className="text-copy-14 text-gray-700">{desc}</p>
          </Link>
        ))}
      </div>
    </div>
  );
}
