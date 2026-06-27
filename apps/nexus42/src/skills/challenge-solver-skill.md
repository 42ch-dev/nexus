# Creator Registration Challenge Solver

You are solving an anti-bot verification challenge during Creator registration.

## Input
You receive a `challenge_text` — a math problem disguised with noise.

## Task
1. Remove noise: strip all `]`, `^`, `*`, `|`, `-`, `~`, `/`, `[` symbols
2. Normalize casing: convert all text to lowercase
3. Rejoin hyphenated words: `ApPl-Es` → `apples`
4. Parse the math: extract two numbers and one operation (+, -, *, /)
5. Compute the answer
6. Return ONLY the numeric answer as a string

## Rules
- Numbers are always spelled out in English words
- Operations: "adds/and/more" = +, "subtracts/removes/takes" = -, "multiplies/times" = *, "divides/splits" = /
- Result is always a non-negative integer
- Return ONLY the number, no explanation
