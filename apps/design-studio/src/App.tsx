import { Link, Routes, Route } from 'react-router-dom';
import { TopNav } from '@/components/nav';
import { ThemeToggle } from '@/components/theme-toggle';
import { HomePage } from '@/pages/home';
import { TokensPage } from '@/pages/tokens';
import { BrandPage } from '@/pages/brand';
import { ComponentsPage } from '@/pages/components';
import { VoicePage } from '@/pages/voice';
import {
  SurfacesAgentPickerPage,
  SurfacesBannerPage,
  SurfacesCanvasPage,
  SurfacesDaemonPage,
  SurfacesIndexPage,
  SurfacesLaunchPage,
  SurfacesLayout,
  SurfacesSelectionSubmenuPage,
  SurfacesSetupPage,
  SurfacesShellPage,
} from '@/pages/surfaces';

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
        <div className="max-w-6xl mx-auto flex items-center justify-between px-4 h-12">
          <div className="flex items-center gap-6">
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
          <Route path="/tokens" element={<TokensPage />} />
          <Route path="/brand" element={<BrandPage />} />
          <Route path="/components" element={<ComponentsPage />} />
          <Route path="/voice" element={<VoicePage />} />
          <Route path="/surfaces" element={<SurfacesLayout />}>
            <Route index element={<SurfacesIndexPage />} />
            <Route path="setup" element={<SurfacesSetupPage />} />
            <Route path="shell" element={<SurfacesShellPage />} />
            <Route path="agent-picker" element={<SurfacesAgentPickerPage />} />
            <Route path="canvas" element={<SurfacesCanvasPage />} />
            <Route path="daemon" element={<SurfacesDaemonPage />} />
            <Route path="launch" element={<SurfacesLaunchPage />} />
            <Route path="banner" element={<SurfacesBannerPage />} />
            <Route path="selection-submenu" element={<SurfacesSelectionSubmenuPage />} />
          </Route>
        </Routes>
      </main>

      {/* Footer — SSOT hint */}
      <footer className="border-t border-gray-alpha-200 py-2 px-4">
        <div className="max-w-6xl mx-auto flex items-center justify-between text-copy-13 text-gray-500">
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
