/**
 * Reading-chrome ReactMarkdown renderers — V1.91 profile-specific read-only
 * chrome.
 *
 * Each shipped work profile (`novel`, `essay`, `game-bible`, `script`) maps
 * markdown elements to distinct, token-driven components. All styling is
 * delegated to DESIGN.md `reading-chrome-*` tokens via the CSS classes in
 * `src/index.css`. This module is intentionally presentation-only and contains
 * no write paths.
 */
import type { Components } from 'react-markdown';

import type { ReadingChromeProfile } from '@/lib/reading-chrome';

/** Props shape that react-markdown passes to custom element overrides. */
export type NodeProps<T extends HTMLElement, Extra = object> = React.HTMLAttributes<T> &
  Extra & { node?: unknown };

export type HeadingProps = NodeProps<
  HTMLHeadingElement,
  { level?: 1 | 2 | 3 | 4 | 5 | 6 }
>;

/**
 * Base paragraph renderer — applies reading typography (line-height + paragraph
 * spacing tokens) to body copy. Used by every profile so the prose column keeps
 * its book-like rhythm.
 */
export function ProseParagraph({ node: _node, ...props }: NodeProps<HTMLParagraphElement>) {
  return (
    <p
      style={{
        lineHeight: 'var(--reading-prose-line-height)',
        marginTop: 'var(--reading-prose-paragraph-spacing)',
      }}
      {...props}
    />
  );
}

/** Build the ReactMarkdown component map for the active chrome profile. */
export function createProfileRenderers(profile: ReadingChromeProfile): Components {
  const base: Components = { p: ProseParagraph };

  switch (profile) {
    case 'novel':
      return {
        ...base,
        h1: NovelChapterTitle,
        hr: NovelSceneSeparator,
        blockquote: NovelEpigraph,
      };
    case 'essay':
      return {
        ...base,
        h2: EssaySectionHeading,
        blockquote: EssayBlockquote,
        a: EssayAnchor,
      };
    case 'game-bible':
      return {
        ...base,
        a: GameBibleTermLink,
        blockquote: GameBibleDefinitionCallout,
      };
    case 'script':
      return {
        ...base,
        h2: ScriptSceneHeading,
        h3: ScriptCharacterName,
        blockquote: ScriptParenthetical,
      };
    default:
      return base;
  }
}

/* ── Profile: novel ── */

function NovelChapterTitle({ node: _node, level: _level, ...props }: HeadingProps) {
  return (
    <h1
      className="reading-chrome-novel-chapter-title"
      data-chrome-element="chapter-title"
      {...props}
    />
  );
}

function NovelSceneSeparator({ node: _node, ...props }: NodeProps<HTMLHRElement>) {
  return (
    <div
      className="reading-chrome-novel-scene-separator"
      data-chrome-element="scene-separator"
      role="separator"
      aria-hidden="true"
      {...props}
    >
      * * *
    </div>
  );
}

function NovelEpigraph({ node: _node, ...props }: NodeProps<HTMLQuoteElement>) {
  return (
    <blockquote
      className="reading-chrome-novel-epigraph"
      data-chrome-element="epigraph"
      {...props}
    />
  );
}

/* ── Profile: essay ── */

function EssaySectionHeading({ node: _node, level: _level, ...props }: HeadingProps) {
  return (
    <h2
      className="reading-chrome-essay-section-heading"
      data-chrome-element="section-heading"
      {...props}
    />
  );
}

function EssayBlockquote({ node: _node, ...props }: NodeProps<HTMLQuoteElement>) {
  return (
    <blockquote
      className="reading-chrome-essay-blockquote"
      data-chrome-element="blockquote"
      {...props}
    />
  );
}

function EssayAnchor({ node: _node, ...props }: NodeProps<HTMLAnchorElement>) {
  if ('data-footnote-ref' in props && props['data-footnote-ref']) {
    return (
      <a
        className="reading-chrome-essay-footnote-marker"
        data-chrome-element="footnote-marker"
        {...props}
      />
    );
  }
  return <a {...props} />;
}

/* ── Profile: game-bible ── */

function GameBibleTermLink({ node: _node, ...props }: NodeProps<HTMLAnchorElement>) {
  return (
    <a
      className="reading-chrome-game-bible-term-link"
      data-chrome-element="term-link"
      {...props}
    />
  );
}

function GameBibleDefinitionCallout({ node: _node, ...props }: NodeProps<HTMLQuoteElement>) {
  return (
    <blockquote
      className="reading-chrome-game-bible-definition-callout"
      data-chrome-element="definition-callout"
      {...props}
    >
      <span className="reading-chrome-game-bible-definition-label">Definition:</span>{' '}
      {props.children}
    </blockquote>
  );
}

/* ── Profile: script ── */

function ScriptSceneHeading({ node: _node, level: _level, ...props }: HeadingProps) {
  return (
    <h2
      className="reading-chrome-script-scene-heading"
      data-chrome-element="scene-heading"
      {...props}
    />
  );
}

function ScriptCharacterName({ node: _node, level: _level, ...props }: HeadingProps) {
  return (
    <h3
      className="reading-chrome-script-character-name"
      data-chrome-element="character-name"
      {...props}
    />
  );
}

function ScriptParenthetical({ node: _node, ...props }: NodeProps<HTMLQuoteElement>) {
  return (
    <blockquote
      className="reading-chrome-script-parenthetical"
      data-chrome-element="parenthetical"
      {...props}
    />
  );
}
