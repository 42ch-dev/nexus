import { lazy, Suspense, type ReactNode } from 'react';
import { Link, Routes, Route } from 'react-router';
import { TopNav } from '@/components/nav';
import { ThemeToggle } from '@/components/theme-toggle';
import { HomePage } from '@/pages/home';

/**
 * Route-level code splitting (S-002): the heavy gallery pages (tokens.tsx
 * ~2.3k lines, brand, components, surfaces subtree) are lazy-loaded so an
 * unrelated route does not eagerly parse every gallery module into the main
 * chunk. Each lazy route keeps the exact label/hash/fixture behavior; the
 * Suspense boundary gives a meaningful, accessible loading state (role +
 * aria-busy). HomePage stays eager as the first route.
 */

const TokensPage = lazy(() => import('@/pages/tokens').then((m) => ({ default: m.TokensPage })));
const BrandPage = lazy(() => import('@/pages/brand').then((m) => ({ default: m.BrandPage })));
const ComponentsPage = lazy(() => import('@/pages/components').then((m) => ({ default: m.ComponentsPage })));
const VoicePage = lazy(() => import('@/pages/voice').then((m) => ({ default: m.VoicePage })));
const SurfacesLayout = lazy(() => import('@/pages/surfaces').then((m) => ({ default: m.SurfacesLayout })));

const SurfacesIndexPage = lazy(() => import('@/pages/surfaces').then((m) => ({ default: m.SurfacesIndexPage })));
const SurfacesSetupPage = lazy(() => import('@/pages/surfaces').then((m) => ({ default: m.SurfacesSetupPage })));
const SurfacesShellPage = lazy(() => import('@/pages/surfaces').then((m) => ({ default: m.SurfacesShellPage })));
const SurfacesAgentPickerPage = lazy(() =>
  import('@/pages/surfaces').then((m) => ({ default: m.SurfacesAgentPickerPage })),
);
const SurfacesCanvasPage = lazy(() => import('@/pages/surfaces').then((m) => ({ default: m.SurfacesCanvasPage })));
const SurfacesDaemonPage = lazy(() => import('@/pages/surfaces').then((m) => ({ default: m.SurfacesDaemonPage })));
const SurfacesLaunchPage = lazy(() => import('@/pages/surfaces').then((m) => ({ default: m.SurfacesLaunchPage })));
const SurfacesSelectionSubmenuPage = lazy(() =>
  import('@/pages/surfaces').then((m) => ({ default: m.SurfacesSelectionSubmenuPage })),
);

/** Accessible loading state shown while a lazy gallery chunk parses. */
function RouteLoading({ label }: { label: string }) {
  return (
    <div
      role="status"
      aria-busy="true"
      data-testid={`route-loading-${label.toLowerCase().replace(/[^a-z]+/g, '-')}`}
      className="max-w-6xl mx-auto py-8 px-4"
    >
      <p className="text-copy-14 text-gray-600">Loading {label}…</p>
    </div>
  );
}

/** Wrap a lazy component in a Suspense boundary with an accessible fallback. */
function lazyRoute(node: ReactNode, label: string) {
  return <Suspense fallback={<RouteLoading label={label} />}>{node}</Suspense>;
}

/**
 * App shell for the Nexus Design Studio.
 *
 * Persistent header with product mark, top nav (5 gallery sections), and
 * theme toggle. Body renders the active route. Footer shows the read-only
 * SSOT hint per IA guide §2.
 *
 * Surfaces uses nested Studio-only section routes (V1.102 P2) — not App
 * Settings IA.
 */
export function App() {
  return (
    <div className="min-h-screen flex flex-col">
      {/* Header chrome */}
      <header className="sticky top-0 z-10 border-b border-gray-alpha-200 bg-background-100/80 backdrop-blur-sm">
        <div className="max-w-6xl mx-auto flex flex-wrap items-center justify-between gap-x-4 gap-y-1 px-4 py-2">
          <div className="flex flex-wrap items-center gap-x-6 gap-y-1">
            <Link to="/" className="text-heading-16 font-semibold text-gray-1000 no-underline hover:opacity-80 transition-opacity">
              Nexus Design Studio
            </Link>
            <TopNav />
          </div>
          <ThemeToggle />
        </div>
      </header>

      {/* Main content */}
      <main className="flex-1">
        <Routes>
          <Route path="/" element={<HomePage />} />
          <Route path="/tokens" element={lazyRoute(<TokensPage />, 'Tokens')} />
          <Route path="/brand" element={lazyRoute(<BrandPage />, 'Brand')} />
          <Route path="/components" element={lazyRoute(<ComponentsPage />, 'Components')} />
          <Route path="/voice" element={lazyRoute(<VoicePage />, 'Voice & Content')} />
          <Route path="/surfaces" element={lazyRoute(<SurfacesLayout />, 'Surfaces')}>
            <Route index element={lazyRoute(<SurfacesIndexPage />, 'Surfaces')} />
            <Route path="setup" element={lazyRoute(<SurfacesSetupPage />, 'Surfaces / Setup')} />
            <Route path="shell" element={lazyRoute(<SurfacesShellPage />, 'Surfaces / Shell')} />
            <Route path="agent-picker" element={lazyRoute(<SurfacesAgentPickerPage />, 'Surfaces / Agent picker')} />
            <Route path="canvas" element={lazyRoute(<SurfacesCanvasPage />, 'Surfaces / Canvas')} />
            <Route path="daemon" element={lazyRoute(<SurfacesDaemonPage />, 'Surfaces / Daemon')} />
            <Route path="launch" element={lazyRoute(<SurfacesLaunchPage />, 'Surfaces / Launch')} />
            <Route path="selection-submenu" element={lazyRoute(<SurfacesSelectionSubmenuPage />, 'Surfaces / Selection')} />
          </Route>
        </Routes>
      </main>

      {/* Footer — SSOT hint */}
      <footer className="border-t border-gray-alpha-200 py-2 px-4">
        <div className="max-w-6xl mx-auto flex items-center justify-between text-copy-13 text-gray-700">
          <span>
            Read-only · edit{' '}
            <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">DESIGN.md</code>
          </span>
          <span>Nexus Design Studio</span>
        </div>
      </footer>
    </div>
  );
}
