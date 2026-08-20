---
module: apps/web
date: 2026-07-05
problem_type: architecture_pattern
category: architecture-patterns
severity: low
applies_when:
  - Adding a new read-only rendering mode that depends on a Work or World classification
  - Extending the manuscript reading surface for a new work_profile
  - Needing to keep the reading surface free of write paths while still feeling profile-native
tags:
  - reading-chrome
  - work-profile
  - design-tokens
  - react-markdown
  - read-only-invariant
  - apps/web
---

# Profile-Aware Reading Chrome

Render the same markdown manuscript body with profile-specific structural cues so the author immediately recognizes the kind of Work they are reading, without introducing any write path into the reading surface.

## Context

Nexus ships several work profiles (`novel`, `essay`, `game-bible`, `script`). The underlying chapter body is plain markdown, but the reading experience should not feel identical across profiles: a novel needs chapter titles and scene separators, a script needs character cues and scene headings, an essay needs section hierarchy and blockquotes, and a game-bible needs term links and definition callouts.

The V1.91 reading-chrome surface had to:

- Stay strictly read-only (no body, outline, or timeline mutation).
- Derive the chrome profile from the existing `Work.work_profile` field.
- Express every profile difference through `apps/web/DESIGN.md` tokens so the UI does not accumulate ad-hoc Tailwind or raw CSS values.
- Be testable with component tests that assert profile-specific attributes rather than snapshotting rendered output.

## Guidance

### 1. Map the external profile value to a chrome-profile key

Backend `work_profile` values may use a different spelling than the token namespace (for example, `game_bible` vs. `game-bible`). Introduce a single normalization helper that every chrome consumer uses:

```ts
// src/lib/reading-chrome.ts
export type ReadingChromeProfile = 'novel' | 'essay' | 'game-bible' | 'script';

export function toReadingChromeProfile(value: string | undefined | null): ReadingChromeProfile {
  if (!value) return 'novel';
  if (value === 'game_bible') return 'game-bible';
  if (isWorkProfile(value)) return value as ReadingChromeProfile;
  return 'novel';
}
```

This isolates the one mapping between wire spelling and DESIGN.md token spelling; every renderer downstream consumes only the chrome-profile key.

### 2. Build a token-driven ReactMarkdown component map

Use `react-markdown` custom `components` to map markdown elements to profile-specific markup. Each mapped component applies only DESIGN.md token classes and `data-chrome-element` attributes. Keep the component factory pure and side-effect-free:

```ts
// src/components/reading/reading-chrome-renderers.tsx
export function createProfileRenderers(profile: ReadingChromeProfile): Components {
  const base: Components = { p: ProseParagraph };
  switch (profile) {
    case 'novel':
      return { ...base, h1: NovelChapterTitle, hr: NovelSceneSeparator, blockquote: NovelEpigraph };
    case 'essay':
      return { ...base, h2: EssaySectionHeading, blockquote: EssayBlockquote, a: EssayAnchor };
    case 'game-bible':
      return { ...base, a: GameBibleTermLink, blockquote: GameBibleDefinitionCallout };
    case 'script':
      return { ...base, h2: ScriptSceneHeading, h3: ScriptCharacterName, blockquote: ScriptParenthetical };
    default:
      return base;
  }
}
```

Token classes follow a strict naming convention so CSS and tests can discover them predictably:

- Container: `data-chrome-profile="<profile>"`
- Element: `className="reading-chrome-<profile>-<element>"` + `data-chrome-element="<element>"`

### 3. Keep the reading surface read-only

The chrome layer must contain no mutation paths. In practice this means:

- No `useMutation`, no `fetch`, no `invoke` inside chrome components.
- No PUT/PATCH/POST routes imported or called from reading components or their tests.
- The renderer factory returns plain React components; it does not receive callbacks that mutate body or outline state.

Make the invariant auditable by wrapping derivation in `useMemo` and keeping all props as read-only data:

```tsx
const profile = useMemo(() => toReadingChromeProfile(workProfile), [workProfile]);
const renderers = useMemo(() => createProfileRenderers(profile), [profile]);
```

### 4. Test profile identity, not pixel output

Component tests should assert the token-derived attributes that prove the correct profile renderer is active:

```tsx
expect(screen.getByRole('region')).toHaveAttribute('data-chrome-profile', 'novel');
expect(document.querySelector('.reading-chrome-novel-chapter-title')).toBeInTheDocument();
```

Avoid snapshotting full rendered markdown; it makes token refactors expensive and does not prove the profile contract.

## Why This Matters

Without this pattern, profile differentiation tends to leak into ad-hoc inline styles or conditional CSS scattered across pages. That makes it hard to add a new profile, hard to keep dark/light themes consistent, and easy to accidentally introduce write paths while "just styling" the reading surface. A token-driven renderer map keeps profile identity, theme identity, and read-only ownership orthogonal.

## When to Apply

- The surface is read-only and derives from `work_profile` (or a similar classification).
- Differences between profiles can be expressed as typography, spacing, separators, or callout treatments.
- The same markdown source is consumed by every profile.

Do not use this pattern for interactive authoring surfaces or for canvas-style spatial layout; those need different ownership models.

## Examples

### Before (avoid)

```tsx
// Profile logic and ad-hoc styles mixed in the page.
<div className={profile === 'novel' ? 'font-serif text-2xl' : 'font-sans text-lg'}>
  <ReactMarkdown>{content}</ReactMarkdown>
</div>
```

### After (preferred)

```tsx
const renderers = createProfileRenderers(toReadingChromeProfile(workProfile));

<div data-chrome-profile={profile} className="rounded-card border ...">
  <div className="reading-prose mx-auto max-w-[var(--reading-prose-measure)]">
    <ReactMarkdown components={renderers}>{bodyContent}</ReactMarkdown>
  </div>
</div>
```

CSS consumes the token names directly:

```css
.reading-chrome-novel-chapter-title {
  font-family: var(--reading-chrome-novel-chapter-title-font-family);
  font-size: var(--reading-chrome-novel-chapter-title-font-size);
}
```

## Related

- `apps/web/DESIGN.md` — `## Reading Chrome` token section (V1.91)
- `apps/web/src/components/reading/reading-prose.tsx`
- `apps/web/src/components/reading/reading-chrome-renderers.tsx`
- `apps/web/src/lib/reading-chrome.ts`
