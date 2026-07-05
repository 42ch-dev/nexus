/**
 * ReadingProse profile-chrome tests — V1.91 P0 headline.
 *
 * Asserts that each shipped work profile (`novel`, `essay`, `game-bible`,
 * `script`) renders with at least two distinct, token-driven visual markers
 * sourced from `apps/web/DESIGN.md` reading-chrome tokens. Unknown profiles
 * fall back to `novel` chrome.
 */
import { describe, expect, it } from 'vitest';

import { ReadingProse } from '@/components/reading/reading-prose';
import { renderInApp } from '@/test/test-providers';
import { screen } from '@testing-library/react';
import type { ChapterBody } from '@42ch/nexus-contracts';

function makeBody(content: string): ChapterBody {
  return {
    work_id: 'w-123',
    chapter: 1,
    volume: 1,
    body_path: 'Works/WRK/Stories/ch01.md',
    content,
    frontmatter: { status: 'draft' },
    read_only: true,
    updated_at: '2026-07-05T00:00:00Z',
  };
}

function renderProse(content: string, workProfile?: string) {
  return renderInApp(
    <ReadingProse
      body={makeBody(content)}
      isLoading={false}
      isError={false}
      onRetry={() => {}}
      workProfile={workProfile}
    />,
  );
}

describe('ReadingProse profile chrome', () => {
  it('renders novel chrome for a missing work_profile', () => {
    renderProse('# Chapter One\n\nBody text.');
    const region = screen.getByRole('region', { name: 'Chapter body' });
    expect(region).toHaveAttribute('data-chrome-profile', 'novel');
    expect(screen.getByRole('heading', { level: 1 })).toHaveClass('reading-chrome-novel-chapter-title');
  });

  it('renders novel chrome for an unknown work_profile', () => {
    renderProse('# Chapter One\n\nBody text.', 'unknown-profile');
    const region = screen.getByRole('region', { name: 'Chapter body' });
    expect(region).toHaveAttribute('data-chrome-profile', 'novel');
  });

  describe('profile: novel', () => {
    it('applies chapter-title and scene-separator tokens', () => {
      renderProse('# Chapter One\n\nFirst scene.\n\n---\n\nSecond scene.', 'novel');
      const region = screen.getByRole('region', { name: 'Chapter body' });
      expect(region).toHaveAttribute('data-chrome-profile', 'novel');

      const title = screen.getByRole('heading', { level: 1 });
      expect(title).toHaveClass('reading-chrome-novel-chapter-title');
      expect(title).toHaveAttribute('data-chrome-element', 'chapter-title');

      const separator = region.querySelector('[data-chrome-element="scene-separator"]');
      expect(separator).toHaveClass('reading-chrome-novel-scene-separator');
      expect(separator).toHaveTextContent('* * *');
    });

    it('applies the epigraph token to blockquotes', () => {
      renderProse('> All happy families are alike.\n\nBody text.', 'novel');
      const epigraph = screen.getByRole('region', { name: 'Chapter body' }).querySelector('blockquote');
      expect(epigraph).toHaveClass('reading-chrome-novel-epigraph');
      expect(epigraph).toHaveAttribute('data-chrome-element', 'epigraph');
    });
  });

  describe('profile: essay', () => {
    it('applies section-heading and blockquote tokens', () => {
      renderProse('## Section One\n\nA paragraph.\n\n> A cited passage.', 'essay');
      const region = screen.getByRole('region', { name: 'Chapter body' });
      expect(region).toHaveAttribute('data-chrome-profile', 'essay');

      const heading = screen.getByRole('heading', { level: 2 });
      expect(heading).toHaveClass('reading-chrome-essay-section-heading');
      expect(heading).toHaveAttribute('data-chrome-element', 'section-heading');

      const quote = screen.getByRole('region', { name: 'Chapter body' }).querySelector('blockquote');
      expect(quote).toHaveClass('reading-chrome-essay-blockquote');
      expect(quote).toHaveAttribute('data-chrome-element', 'blockquote');
    });

    it('applies the footnote-marker token to footnote references', () => {
      renderProse('A claim.[^1]\n\n[^1]: The supporting note.', 'essay');
      const region = screen.getByRole('region', { name: 'Chapter body' });
      const marker = region.querySelector('a[data-footnote-ref]');
      expect(marker).toHaveClass('reading-chrome-essay-footnote-marker');
      expect(marker).toHaveAttribute('data-chrome-element', 'footnote-marker');
    });
  });

  describe('profile: game-bible', () => {
    it('applies term-link and definition-callout tokens', () => {
      renderProse('[Aether](aether)\n\n> The fundamental force.', 'game_bible');
      const region = screen.getByRole('region', { name: 'Chapter body' });
      expect(region).toHaveAttribute('data-chrome-profile', 'game-bible');

      const link = screen.getByRole('link', { name: 'Aether' });
      expect(link).toHaveClass('reading-chrome-game-bible-term-link');
      expect(link).toHaveAttribute('data-chrome-element', 'term-link');

      const callout = region.querySelector('blockquote');
      expect(callout).toHaveClass('reading-chrome-game-bible-definition-callout');
      expect(callout).toHaveAttribute('data-chrome-element', 'definition-callout');
      expect(callout).toHaveTextContent('Definition:');
    });

    it('maps the wire value game_bible to the game-bible chrome profile', () => {
      renderProse('[Aether](aether)', 'game_bible');
      const region = screen.getByRole('region', { name: 'Chapter body' });
      expect(region).toHaveAttribute('data-chrome-profile', 'game-bible');
    });
  });

  describe('profile: script', () => {
    it('applies scene-heading, character-name, and parenthetical tokens', () => {
      renderProse('## INT. COFFEE SHOP - DAY\n\n### ALICE\n\n> (sotto)\n\nLine.', 'script');
      const region = screen.getByRole('region', { name: 'Chapter body' });
      expect(region).toHaveAttribute('data-chrome-profile', 'script');

      const scene = region.querySelector('[data-chrome-element="scene-heading"]');
      expect(scene).toHaveClass('reading-chrome-script-scene-heading');
      expect(scene).toHaveAttribute('data-chrome-element', 'scene-heading');

      const character = region.querySelector('[data-chrome-element="character-name"]');
      expect(character).toHaveClass('reading-chrome-script-character-name');
      expect(character).toHaveAttribute('data-chrome-element', 'character-name');

      const parenthetical = region.querySelector('blockquote');
      expect(parenthetical).toHaveClass('reading-chrome-script-parenthetical');
      expect(parenthetical).toHaveAttribute('data-chrome-element', 'parenthetical');
    });
  });
});
