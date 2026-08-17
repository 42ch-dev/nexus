# Reading Chrome Profile Checklist

> **Version**: V1.91 — locked in P-1 Prepare.
> **Source of truth**: repo-root [`DESIGN.md`](../../DESIGN.md) `## Reading Chrome` token section.
> **Purpose**: Acceptance bar for P0 implementation of profile-specific reading chrome.
> **Fallback rule**: unknown `work_profile` values render as `novel` chrome.

## Overview

Each shipped `work_profile` must render with ≥2 distinct, token-driven visual markers sourced exclusively from `DESIGN.md` `reading-chrome-*` tokens. No ad-hoc CSS or Tailwind utilities for profile differentiation.

## Profile: `novel`

| # | Chrome element | DESIGN.md token | Acceptance criterion |
|---|---------------|-----------------|---------------------|
| 1 | Chapter/epigraph title | `reading-chrome-novel.chapter-title` | Chapter titles render in Georgia serif at 28px/700 weight, visually distinct from body copy. Epigraph text styled identically if present. |
| 2 | Scene separator | `reading-chrome-novel.scene-separator` | Scene breaks between sections of a chapter render a centered `* * *` separator in gray-500 at 16px, with 12px vertical padding. Absent when no scene break exists. |
| 3 | Epigraph block | `reading-chrome-novel.epigraph` | Epigraph text (when present) renders italic, right-aligned, in gray-700, indented from the left by 25%. |

## Profile: `essay`

| # | Chrome element | DESIGN.md token | Acceptance criterion |
|---|---------------|-----------------|---------------------|
| 1 | Section heading | `reading-chrome-essay.section-heading` | Section-level headings render at 18px/500 weight with 28px top margin, distinct from body copy (e.g., copy-16 at 400 weight). |
| 2 | Styled blockquote | `reading-chrome-essay.blockquote` | Blockquotes render with a 3px left border in gray-alpha-400, italic text in gray-900, and 16px left padding. |
| 3 | Footnote marker | `reading-chrome-essay.footnote-marker` | Inline footnote reference numbers render as superscript at 0.75em in teal-700. |

## Profile: `game-bible`

| # | Chrome element | DESIGN.md token | Acceptance criterion |
|---|---------------|-----------------|---------------------|
| 1 | Term cross-reference link | `reading-chrome-game-bible.term-link` | Defined world terms that link to entity details render with a dotted underline in teal-700, visually distinct from standard hyperlinks. |
| 2 | Definition callout | `reading-chrome-game-bible.definition-callout` | Definition blocks render with a 3px teal left border, light teal background tint (rgba(0,133,119,0.06)), and the `Definition:` label prefix in teal-900/600 weight. |
| 3 | Category badge | `reading-chrome-game-bible.category-badge` | Category labels on entity cards (e.g., "Character", "Location") render as pills with amber background tint and amber-1000 text. |

## Profile: `script`

| # | Chrome element | DESIGN.md token | Acceptance criterion |
|---|---------------|-----------------|---------------------|
| 1 | Character-name block | `reading-chrome-script.character-name` | Character cues above dialogue render centered, uppercase, bold at 14px with 0.08em letter-spacing and 20px top margin. |
| 2 | Parenthetical direction | `reading-chrome-script.parenthetical` | Parenthetical stage directions within dialogue render italic in gray-700, indented 32px from the left. |
| 3 | Scene heading | `reading-chrome-script.scene-heading` | Scene/slug lines (INT./EXT. ...) render uppercase, bold at 14px with 0.05em letter-spacing in gray-900, and 24px top margin. |

## Verification

- Each profile renders with its chrome tokens when the respective `work_profile` is active.
- Unknown `work_profile` values fall back to `novel` chrome (default).
- No token names from this section are renamed or removed.
- Component tests assert profile-specific attributes (e.g., `data-chrome-profile="novel"`).
- PR includes at least one visual artifact (screenshot or storybook capture) per profile.
