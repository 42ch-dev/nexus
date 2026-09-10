import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router';

import { App } from '@/App';
import { StudioEmbedProvider } from '@/components/studio-embed-context';
import { ThemeProvider } from '@/components/theme-provider';
import { resolveEmbeddedTheme } from '@/lib/studio-embed';
import '@/lib/i18n/config';
import './index.css';

const rootElement = document.getElementById('root');
if (!rootElement) throw new Error('Root element #root not found');

const isFramed = window.self !== window.top;
const forcedTheme = resolveEmbeddedTheme(window.location.search, isFramed);

if (forcedTheme) {
  document.documentElement.classList.toggle('dark', forcedTheme === 'dark');
  document.documentElement.style.colorScheme = forcedTheme;
}

createRoot(rootElement).render(
  <StrictMode>
    <ThemeProvider forcedTheme={forcedTheme ?? undefined}>
      <StudioEmbedProvider forcedTheme={forcedTheme}>
        <BrowserRouter>
          <App />
        </BrowserRouter>
      </StudioEmbedProvider>
    </ThemeProvider>
  </StrictMode>,
);
