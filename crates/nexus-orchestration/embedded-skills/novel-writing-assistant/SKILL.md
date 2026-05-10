# Novel Writing Assistant

## Purpose

This skill equips an ACP agent with structured guidance for novel-writing sessions.
It is designed to be injected into the agent's context when the `novel-writing`
orchestration preset is active.

## What the skill provides

### Story structure

- Three-act, five-act, and non-linear structure templates.
- Milestone checkpoints (inciting incident, midpoint reversal, climax, resolution).
- Scene-level beats: goal → conflict → disaster / yes-but / no-and.

### Chapter drafting

- Opening hooks and closing anchors per chapter.
- Pacing guidance: scene vs. sequel rhythm (action → processing → decision).
- Recommended chapter word-count ranges by genre.

### Character consistency

- Character sheet template (name, motivation, arc, voice markers, relationships).
- Cross-chapter consistency checks: speech patterns, knowledge boundaries, behavioural tics.
- Relationship evolution tracking (allies → rivals, trust gained/lost per scene).

### Narrative pacing

- Tension curve mapping across the manuscript.
- Variation strategies: alternation of fast (dialogue/action) and slow (reflection/worldbuilding) passages.
- Callback and foreshadowing placement heuristics.

## Usage

The agent should reference the relevant section at each orchestration state:

| Orchestration state | Applicable section |
|---|---|
| `gathering` | Character sheet template |
| `brainstorming` | Story structure templates, milestone checkpoints |
| `outlining` | Scene-level beats, tension curve mapping |
| `drafting` | Chapter drafting guidance, pacing, character consistency checks |

## Version

1 — initial embedded release.
