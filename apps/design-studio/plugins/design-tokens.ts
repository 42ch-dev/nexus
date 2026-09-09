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
 *  - load(id) returns the freshly compiled CSS string directly, so the plugin
 *    graph serves projected content instead of the derived disk artifact.
 *  - transform() on any CSS module replaces literal
 *      @import '@nexus/design-tokens/tokens.css';
 *      @import '@42ch/nexus-ui/theme.css';
 *    (as they appear in apps/design-studio/src/index.css) with the freshly
 *    compiled CSS inline. postcss-import would otherwise inline the checked-in
 *    disk file; this rewrites the source import so dev always reflects DESIGN.
 *    The pair is re-read on every request, so a manual reload always reflects
 *    the current DESIGN pair even without a watch event.
 *  - Registers repo-root DESIGN.md + DESIGN.dark.md as watch inputs; on either
 *    change it invalidates both CSS module graphs and full-reloads so
 *    computed-value labels also refresh. No HMR source writes.
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

  /** Load + compile the pair; throws on malformed input (→ Vite overlay). */
  async function compile(): Promise<{ css: string; brandCss: string }> {
    const pair = await loadDesignPair(designRoot);
    const out = projectDesign(pair);
    return { css: out.css, brandCss: out.brandCss };
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
      return (await compile())[field];
    },

    async transform(code, id) {
      if (!id.endsWith('.css')) return null;
      if (!code.includes(TOKENS_SPEC) && !code.includes(THEME_SPEC)) return null;
      const fresh = await compile();
      let out = code;
      out = out.replace(new RegExp(`@import\\s+['"]${TOKENS_SPEC}['"];?`), fresh.css.trimEnd());
      out = out.replace(new RegExp(`@import\\s+['"]${THEME_SPEC}['"];?`), fresh.brandCss.trimEnd());
      return out;
    },

    config() {
      return {
        server: {
          watch: {
            ignored: (path: string) => {
              const abs = path.startsWith('/') ? path : join(designRoot, path);
              return !(abs === designFile || abs === darkFile);
            },
          },
        },
      };
    },

    configureServer(server) {
      const invalidate = () => {
        for (const virtual of [TOKENS_VIRTUAL, THEME_VIRTUAL]) {
          for (const mod of server.moduleGraph.getModulesByFile(virtual) || []) {
            server.moduleGraph.invalidateModule(mod);
          }
        }
        // The entry CSS module (src/index.css) carries the transform output
        // that embeds the compiled tokens inline — invalidate it too so its
        // transform re-runs on the next request and reflects the DESIGN edit
        // after the full reload.
        for (const file of server.moduleGraph.filesById?.keys() || []) {
          if (file.endsWith('.css')) {
            for (const mod of server.moduleGraph.getModulesByFile(file) || []) {
              server.moduleGraph.invalidateModule(mod);
            }
          }
        }
        server.ws.send({ type: 'full-reload' });
      };
      const watcher = server.watcher as unknown as { on(e: string, cb: (p: string) => void): void };
      if (watcher && typeof watcher.on === 'function') {
        for (const ev of ['change', 'add', 'unlink'] as const) {
          watcher.on(ev, (path: string) => {
            if (path === designFile || path === darkFile) invalidate();
          });
        }
      }
    },
  };
}
