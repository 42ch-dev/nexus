import { lazy, Suspense, useEffect, useRef, type ReactNode } from 'react';
import { Link, Routes, Route } from 'react-router';

import { EmbeddedRouteReady } from '@/components/embed-ready-notifier';
import { GalleryShell } from '@/components/gallery-shell';
import { TopNav } from '@/components/nav';
import { StudioShellLogo } from '@/components/studio-shell-logo';
import { useStudioEmbed } from '@/components/studio-embed-context';
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
      aria-label={`Loading ${label}`}
      data-testid={`route-loading-${label.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`}
      className="max-w-6xl mx-auto py-16 px-4 text-copy-16 text-gray-700"
    >
      Loading {label}…
    </div>
  );
}

/** Wrap a lazy component in a Suspense boundary with an accessible fallback. */
function lazyRoute(node: ReactNode, label: string) {
  return <Suspense fallback={<RouteLoading label={label} />}>{node}</Suspense>;
}

/** Embedded layout routes defer readiness to nested leaf routes. */
function embeddedLazyLayoutRoute(node: ReactNode, label: string) {
  return <Suspense fallback={<RouteLoading label={label} />}>{node}</Suspense>;
}

/** Embedded routes post ready only after the lazy gallery page commits. */
function embeddedLazyRoute(node: ReactNode, label: string) {
  return (
    <Suspense fallback={<RouteLoading label={label} />}>
      <EmbeddedRouteReady>{node}</EmbeddedRouteReady>
    </Suspense>
  );
}

function EmbeddedRoutes() {
  return (
    <Routes>
      <Route path="/tokens" element={embeddedLazyRoute(<TokensPage />, 'Tokens')} />
      <Route path="/brand" element={embeddedLazyRoute(<BrandPage />, 'Brand')} />
      <Route path="/components" element={embeddedLazyRoute(<ComponentsPage />, 'Components')} />
      <Route path="/voice" element={embeddedLazyRoute(<VoicePage />, 'Voice & Content')} />
      <Route path="/surfaces" element={embeddedLazyLayoutRoute(<SurfacesLayout />, 'Surfaces')}>
        <Route index element={embeddedLazyRoute(<SurfacesIndexPage />, 'Surfaces')} />
        <Route path="setup" element={embeddedLazyRoute(<SurfacesSetupPage />, 'Surfaces / Setup')} />
        <Route path="shell" element={embeddedLazyRoute(<SurfacesShellPage />, 'Surfaces / Shell')} />
        <Route
          path="agent-picker"
          element={embeddedLazyRoute(<SurfacesAgentPickerPage />, 'Surfaces / Agent picker')}
        />
        <Route path="canvas" element={embeddedLazyRoute(<SurfacesCanvasPage />, 'Surfaces / Canvas')} />
        <Route path="daemon" element={embeddedLazyRoute(<SurfacesDaemonPage />, 'Surfaces / Daemon')} />
        <Route path="launch" element={embeddedLazyRoute(<SurfacesLaunchPage />, 'Surfaces / Launch')} />
        <Route
          path="selection-submenu"
          element={embeddedLazyRoute(<SurfacesSelectionSubmenuPage />, 'Surfaces / Selection')}
        />
      </Route>
    </Routes>
  );
}

function withGalleryShell(node: ReactNode) {
  return <GalleryShell>{node}</GalleryShell>;
}

function GalleryRoutes() {
  return (
    <Routes>
      <Route path="/" element={<HomePage />} />
      <Route path="/tokens" element={lazyRoute(withGalleryShell(<TokensPage />), 'Tokens')} />
      <Route path="/brand" element={lazyRoute(withGalleryShell(<BrandPage />), 'Brand')} />
      <Route
        path="/components"
        element={lazyRoute(withGalleryShell(<ComponentsPage />), 'Components')}
      />
      <Route path="/voice" element={lazyRoute(withGalleryShell(<VoicePage />), 'Voice & Content')} />
      <Route
        path="/surfaces"
        element={lazyRoute(withGalleryShell(<SurfacesLayout />), 'Surfaces')}
      >
        <Route index element={lazyRoute(<SurfacesIndexPage />, 'Surfaces')} />
        <Route path="setup" element={lazyRoute(<SurfacesSetupPage />, 'Surfaces / Setup')} />
        <Route path="shell" element={lazyRoute(<SurfacesShellPage />, 'Surfaces / Shell')} />
        <Route
          path="agent-picker"
          element={lazyRoute(<SurfacesAgentPickerPage />, 'Surfaces / Agent picker')}
        />
        <Route path="canvas" element={lazyRoute(<SurfacesCanvasPage />, 'Surfaces / Canvas')} />
        <Route path="daemon" element={lazyRoute(<SurfacesDaemonPage />, 'Surfaces / Daemon')} />
        <Route path="launch" element={lazyRoute(<SurfacesLaunchPage />, 'Surfaces / Launch')} />
        <Route
          path="selection-submenu"
          element={lazyRoute(<SurfacesSelectionSubmenuPage />, 'Surfaces / Selection')}
        />
      </Route>
    </Routes>
  );
}

/**
 * App shell for the Nexus Design Studio.
 *
 * Persistent header with product mark, top nav (5 gallery sections), and
 * theme toggle. Body renders the active route. Footer shows the read-only
 * SSOT hint per IA guide §2.
 *
 * Surfaces uses nested Studio-only section routes (V1.102 P2) — not App
 * Settings IA. Embedded iframe documents omit chrome, discovery, comparison,
 * and the Surfaces navigation rail.
 */
export function App() {
  const { isEmbedded } = useStudioEmbed();
  const headerRef = useRef<HTMLElement>(null);

  useEffect(() => {
    if (isEmbedded) return;
    const header = headerRef.current;
    if (!header) return;

    const syncStickyHeaderOffset = () => {
      document.documentElement.style.setProperty(
        '--studio-sticky-header-offset',
        `${header.offsetHeight}px`,
      );
    };

    syncStickyHeaderOffset();

    if (typeof ResizeObserver === 'undefined') {
      return;
    }

    const observer = new ResizeObserver(syncStickyHeaderOffset);
    observer.observe(header);
    return () => observer.disconnect();
  }, [isEmbedded]);

  if (isEmbedded) {
    return (
      <div className="min-h-screen bg-background-100">
        <EmbeddedRoutes />
      </div>
    );
  }

  return (
    <div className="min-h-screen flex flex-col">
      <header
        ref={headerRef}
        className="sticky top-0 z-20 min-h-12 border-b border-gray-alpha-200 bg-background-100/90 backdrop-blur-sm"
      >
        <div className="mx-auto flex min-h-12 max-w-6xl flex-wrap items-center justify-between gap-x-4 gap-y-1 px-4 py-1">
          <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1">
            <Link
              to="/"
              className="flex min-w-0 items-center gap-2 no-underline hover:opacity-80 transition-opacity"
            >
              <StudioShellLogo />
              <span className="truncate text-heading-16 font-semibold text-gray-1000">
                Design Studio
              </span>
            </Link>
            <TopNav />
          </div>
          <ThemeToggle />
        </div>
      </header>

      <main className="flex-1">
        <GalleryRoutes />
      </main>

      <footer className="border-t border-gray-alpha-200 py-2 px-4">
        <div className="max-w-6xl mx-auto flex flex-wrap items-center justify-between gap-2 text-copy-13 text-gray-700">
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

