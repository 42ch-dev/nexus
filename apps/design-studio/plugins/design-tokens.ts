/**
 * Design Studio Vite plugin — in-memory DESIGN token projection.
 *
 * In `vite dev` this plugin rewrites the two shared CSS modules
 * (`@nexus/design-tokens/tokens.css` and `@42ch/nexus-ui/theme.css`) in memory
 * using the same compiler that produces the checked-in build artifacts
 * (tooling/design-tokens/scripts/project-tokens.mjs). It never writes source
 * files during HMR and never bundles raw YAML to the browser.
 *
 * Model:
 *  - resolveId maps each bare shared-CSS specifier to a stable virtual id.
 *  - load(id) / transform() return the freshly compiled CSS string, but the
 *    compiled pair is memoized so one page load compiles the DESIGN pair at
 *    most once (W-001) instead of once per load + transform. The memo is
 *    reset on DESIGN watch events and on document-request manual navigation.
 *  - transform() on any CSS module replaces literal
 *      @import '@nexus/design-tokens/tokens.css';
 *      @import '@42ch/nexus-ui/theme.css';
 *    (as they appear in apps/design-studio/src/index.css) with the compiled
 *    CSS inline. postcss-import would otherwise inline the checked-in disk
 *    file; this rewrites the source import so dev always reflects DESIGN.
 *  - On a document (HTML) request we invalidate the CSS module graphs (and
 *    reset the compile cache) WITHOUT sending `full-reload` (which would loop
 *    on that request path). This makes a manual reload re-read the current
 *    DESIGN pair even if no watcher event fired (F-002, spec §3.5).
 *  - Registers repo-root DESIGN.md + DESIGN.dark.md as watch inputs; on either
 *    change it invalidates both CSS module graphs and full-reloads so
 *    computed-value labels also refresh, and resets the cache. Watcher paths
 *    are normalized before equality checks. No HMR source writes. The default
 *    Vite source/dependency watcher for apps/design-studio is left intact, so
 *    ordinary React/CSS source edits keep triggering normal HMR.
 *  - Malformed DESIGN frontmatter throws inside load/transform → Vite overlay.
 */
import { join, resolve } from 'node:path';
import type { Plugin } from 'vite';
import {
  loadDesignPair,
  projectDesign,
} from '../../../tooling/design-tokens/scripts/project-tokens.mjs';

const TOKENS_SPEC = '@nexus/design-tokens/tokens.css';
const THEME_SPEC = '@42ch/nexus-ui/theme.css';
const TOKENS_VIRTUAL = `/virtual/${TOKENS_SPEC}`;
const THEME_VIRTUAL = `/virtual/${THEME_SPEC}`;

const SPEC_TO_VIRTUAL: Record<string, string> = {
  [TOKENS_SPEC]: TOKENS_VIRTUAL,
  [THEME_SPEC]: THEME_VIRTUAL,
};
const VIRTUAL_TO_FIELD: Record<string, 'css' | 'brandCss'> = {
  [TOKENS_VIRTUAL]: 'css',
  [THEME_VIRTUAL]: 'brandCss',
};

/**
 * Build the dev-only Vite plugin.
 * @param repoRoot repository root (where DESIGN.md / DESIGN.dark.md live)
 */
export function designTokensPlugin(repoRoot: string): Plugin {
  const designRoot = resolve(repoRoot);
  const designFile = join(designRoot, 'DESIGN.md');
  const darkFile = join(designRoot, 'DESIGN.dark.md');

  // Memoize one compiled pair so a single page (load + transform + nested
  // imports) compiles at most once (W-001). The cache is reset explicitly on
  // DESIGN watcher events and on document-request (manual-navigation) refresh,
  // so a reload always re-reads the current pair without a stat per request.
  let compileCache: { css: string; brandCss: string } | null = null;

  /** Load + compile the pair; throws on malformed input (→ Vite overlay). */
  async function compile(): Promise<{ css: string; brandCss: string }> {
    const pair = await loadDesignPair(designRoot);
    const out = projectDesign(pair);
    return { css: out.css, brandCss: out.brandCss };
  }

  /** Return the memoized pair, recompiling once when the cache is stale. */
  async function compiled(): Promise<{ css: string; brandCss: string }> {
    compileCache ??= await compile();
    return compileCache;
  }

  return {
    name: 'design-tokens',
    enforce: 'pre',

    resolveId(id) {
      return SPEC_TO_VIRTUAL[id] ?? null;
    },

    async load(id) {
      const field = VIRTUAL_TO_FIELD[id];
      if (!field) return null;
      return (await compiled())[field];
    },

    async transform(code, id) {
      if (!id.endsWith('.css')) return null;
      if (!code.includes(TOKENS_SPEC) && !code.includes(THEME_SPEC)) return null;
      const fresh = await compiled();
      let out = code;
      out = out.replace(new RegExp(`@import\\s+['"]${TOKENS_SPEC}['"];?`), fresh.css.trimEnd());
      out = out.replace(new RegExp(`@import\\s+['"]${THEME_SPEC}['"];?`), fresh.brandCss.trimEnd());
      return out;
    },

    configureServer(server) {
      const invalidateCssGraph = () => {
        // Invalidate every CSS module in the graph by id — including the two
        // virtual shared-CSS modules (keyed by virtual id, not a real file,
        // so getModulesByFile/filesById miss them) and the Studio entry
        // src/index.css whose cached transform output embeds the compiled
        // tokens. Without invalidating the entry module, a reload re-fetches
        // the stale transform result and the DESIGN edit is lost.
        for (const [id, mod] of server.moduleGraph.idToModuleMap ?? []) {
          if (
            id === TOKENS_VIRTUAL ||
            id === THEME_VIRTUAL ||
            (typeof id === 'string' && id.endsWith('.css'))
          ) {
            server.moduleGraph.invalidateModule(mod);
          }
        }
      };

      const invalidate = () => {
        compileCache = null;
        invalidateCssGraph();
        server.ws.send({ type: 'full-reload' });
      };

      // Re-read the pair on any document (HTML) navigation: invalidate the CSS
      // graph and the compile cache WITHOUT a full-reload (sending one from
      // this request path would loop — the reload that already happened is the
      // refresh). This is the missed-watch safety net: a manual reload picks
      // up the current DESIGN pair even if chokidar never fired.
      server.middlewares.use((req, _res, next) => {
        if (
          (req.method === 'GET' || req.method === 'HEAD') &&
          req.headers.accept?.includes('text/html')
        ) {
          compileCache = null;
          invalidateCssGraph();
        }
        next();
      });

      // DESIGN.md / DESIGN.dark.md live at the repo root, which is outside
      // Vite's watch root (apps/design-studio). The server.watcher only
      // reports changes it is configured to watch; add the DESIGN pair
      // explicitly so edits to them invalidate the CSS graphs and trigger a
      // full reload. Without this the transform output stays cached and stale
      // on every request (including manual reload / cold tabs).
      server.watcher.add([designFile, darkFile]);
      const watcher = server.watcher as unknown as { on(e: string, cb: (p: string) => void): void };
      if (watcher && typeof watcher.on === 'function') {
        for (const ev of ['change', 'add', 'unlink'] as const) {
          watcher.on(ev, (path: string) => {
            const normalized = resolve(path);
            if (normalized === designFile || normalized === darkFile) invalidate();
          });
        }
      }
    },
  };
}
