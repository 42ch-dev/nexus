# Spec: VI atmosphere — Chronos lock

**plan_id:** `2026-07-22-v1.130-p4-vi-atmosphere`  
**Status:** specify+clarify+plan locked (architect Seat 2)  
**Wave:** 1∥ (token lock before P1 App paint)

## Problem

`brand-deep-blue #1E3A5F` reads as traditional office software. Author wants the shell to feel 生机 / 神秘 / 时间流 while keeping the cyan accent `#25D1E0` they trust. The current atmosphere actively fights the creative-writing product identity.

## Goals

- Studio candidates T1 Chronos / T2 Umbra / T3 Aurora void; **default lock T1 Chronos** (compile-time, not runtime)
- Re-hue `brand-deep-blue*` + dark atmosphere casts; **keep token names**; preserve cyan hex `#25D1E0`
- Update DESIGN.md / DESIGN.dark.md / tokens.css / Tokens gallery; App+Studio consume
- Reference corpus: https://github.com/voltagent/awesome-design-md (ElevenLabs, Runway, VoltAgent, etc.) — distill patterns, do not replace Nexus DESIGN wholesale

## Non-Goals

- Runtime multi-theme switcher (T1 is compile-time default; T2/T3 Studio-pick only); third competing bright accent; purple-neon AI clichés

## Architecture decision (locked)

- T1 Chronos is compile-time token data behind the existing public token names. No atmosphere id, runtime provider branch, preference, storage key, feature flag, or switcher is introduced.
- T2 Umbra and T3 Aurora are Studio-local comparison swatches only. They are not exported through DESIGN, design-tokens, or `@42ch/nexus-ui`.
- Projection order is mandatory: Studio comparison fixture → locked T1 in `DESIGN.md` / `DESIGN.dark.md` → existing `@42ch/nexus-ui` brand token/theme mirror → `@nexus/design-tokens` CSS/Tailwind projection → Studio Tokens gallery → App smoke.
- `brand-cyan` remains exactly `#25D1E0`. Names such as `brand-deep-blue*` and `blue-*` remain stable for compatibility, but the `#1E3A5F` office-navy value/casts are removed from product shell chrome and runtime projections.
- Every changed text/fill, focus, active, logo, and shell pairing is measured in light and dark: normal text ≥4.5:1; large text/graphical affordances ≥3:1.
- Shell chrome is a web presentational extract shown in Studio via `@web-layout/*`. This plan promotes no new `@42ch/nexus-ui` primitive; package changes only synchronize the existing public brand token mirror.

## Wire

- `wire_contracts_changed: false`

## Acceptance

Per compass AC **VI (P4)** section. Plan-level DoD maps T1–T4 → AC VI. AA contrast documented light + dark.

## Risks

Contrast regressions on light primary; logo mark contrast on new deep.
