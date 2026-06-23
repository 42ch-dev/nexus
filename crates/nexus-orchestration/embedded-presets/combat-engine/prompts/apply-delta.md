# Combat Engine — Apply Delta

The compute module's state deltas have been applied to the world's KeyBlocks.

## Applied Delta Summary
The engine applied the following operations:
- **HP changes:** characters gained or lost hit points based on combat resolution.
- **Status effects:** any new buffs, debuffs, or status conditions have been written.
- **Position changes:** characters may have moved between front_line/back_line.

## Instructions
1. Verify that no character has negative HP without the `is_alive: false` flag.
2. Check that all `computable: true` KeyBlocks still have valid `state` objects.
3. The next stage (`advance_timeline`) will record these changes as narrative events.

## Output
Report the final state of all combatants after the delta has been applied.
