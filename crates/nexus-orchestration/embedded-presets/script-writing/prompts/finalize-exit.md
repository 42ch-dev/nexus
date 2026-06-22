---
vars:
  work_ref: { type: string, required: true }
  title: { type: string, default: "" }
---

# Script 五问 Review — {{work_ref}}

You are reviewing a script for quality. The script has been outlined, drafted, and revised.
Your job is to evaluate it against **five quality dimensions** and respond with
**GO** (the script passes) or **NOGO** (the script needs revision).

## The Five Questions

Answer each with **YES** or **NO**, plus a one-sentence justification.
If all five are YES, respond with GO. Otherwise, respond with NOGO.

### 1. Dialogue Coherence
Is every line of dialogue character-appropriate and narratively motivated?
- Each character speaks in a distinct, consistent voice (YES/NO)?
- No line is pure exposition without dramatic purpose (YES/NO)?
- Every exchange advances plot, reveals character, or builds tension (YES/NO)?

### 2. Beat Pacing
Does each beat land at the right moment and drive the scene forward?
- Beats follow a recognizable pattern: setup → conflict → turn (YES/NO)?
- No beat overstays its welcome (YES/NO)?
- No beat is skipped — emotional transitions are earned (YES/NO)?

### 3. Act Structure
Does each act follow a recognizable structural arc?
- Acts have clear beginning, middle, and end (YES/NO)?
- Act breaks are narratively motivated, not arbitrary (YES/NO)?
- The audience can tell where they are in the narrative journey (YES/NO)?

### 4. Character Voice
Is each character's voice distinct, consistent, and true to their traits?
- No two characters sound identical (YES/NO)?
- Vocabulary, sentence length, and register vary appropriately by character (YES/NO)?
- Voice remains stable across scenes; character development is gradual (YES/NO)?

### 5. Scene Economy
Is every scene necessary? Does every scene earn its place in the narrative?
- No redundant or filler scenes (YES/NO)?
- Each scene advances plot, deepens character, or enriches world (YES/NO)?
- Every scene could justify its inclusion to a skeptical editor (YES/NO)?

## Response Format

Respond with exactly one of:
- `GO` — if all five questions are YES
- `NOGO: <reason>` — if any question is NO, with a one-line reason identifying the failed dimension(s)

Examples:
- `GO`
- `NOGO: dialogue coherence (Alice and Bob sound identical in Act 2) + scene economy (bar scene is pure exposition, can be cut)`
