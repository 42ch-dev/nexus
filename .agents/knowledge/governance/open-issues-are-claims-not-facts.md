---
title: An open issue is a claim about the past, not a statement about the code
slug: open-issues-are-claims-not-facts
kind: lesson
status: active
applies_to: [governance, triage, ledger]
---

# An open issue is a claim about the past, not a statement about the code

Disposition `open` records that nobody has closed the row. It does not record that the defect is still reproducible. In one iteration, seven of ten high-severity issues were already fixed on `main` — some for days, one for two minor lines.

The failure mode compounds: an analyst reads the ledger, concludes the work is blocked, and schedules a round around it. The round then discovers the work is done, which reads as the analyst having been wrong about everything, which makes the next reading of the ledger less trustworthy.

## Verify the claim, not the disposition

For each candidate issue, before scheduling anything on it:

1. Read `observedBehavior` and find the symbol it names.
2. Open that symbol at HEAD.
3. Decide: still reproducible, already fixed, or unreproducible.

An issue fixed on `main` but absent from the branch you are about to work on is a merge problem, not a closed issue — check ancestry (`git merge-base --is-ancestor <fix> <base>`), not just presence.

## Do not hand-write the open predicate

`disposition NOT IN ('resolved','closed','duplicate','wontfix')` returned 178 rows. The CLI returned 172. The six-row difference was `waived` — deliberately out of scope, and excluded by the tool already.

Use the tool's own filter (`--disposition open`). A hand-written predicate silently redefines the vocabulary the tool owns, and the count it produces is not the count anyone else sees.

## What a correct ledger read looks like

- `--disposition open` for "who is still claiming a defect"
- a symbol check for "is that claim still true"
- a fixing-commit citation in the closure reason, so the next reader can date the fix rather than trusting the disposition

The last part matters more than it looks: a closure reason that says "fixed in code" without a commit is indistinguishable from a closure reason written by someone who wanted the row gone.
