---
title: A keeper must assert the value, not the behaviour's name
category: testing-patterns
track: knowledge
last_updated: 2026-11-22
created: 2026-11-22
date: 2026-11-22
status: active
problem_type: testing_pattern
module: repo-wide test corpus
severity: medium
symptoms:
  - "a test is deleted because 'the owner test covers it', and the owner test asserts only the error variant"
  - "a field-level payload (expected/actual, a source string, a name) loses its only assertion while the suite stays green"
  - "a review round declares the diff clean, then a second round with fresh eyes finds two more gaps of the same class"
root_cause: "Naming a keeper is cheap; proving it asserts the same value is not. Reviewers and implementers both reach for a grep hit and stop at the file:line, and a keeper that matches `Err(Variant { .. })` looks identical to one that matches `Err(Variant { expected: 5, actual: 3 })` in a table."
resolution_type: workflow_improvement
---

# A keeper must assert the value, not the behaviour's name

## Context

A 1,515-line test deletion across three plans looked clean twice. Six review rounds later, four separate keepers had been shown to assert something weaker than what they replaced:

- `test_delete_keyblock` created a fresh record and deleted it, never covering delete from `confirmed` or `deprecated`.
- `test_merge_keyblock` covered only success, not the rejection path's post-call status.
- The revision-mismatch keeper matched `KbError::RevisionMismatch { .. }`, so swapping `expected` and `actual` would pass.
- `map_spoke_reject`'s `Reject` arm had no test at all; the surviving assertion checked a spoke-table boolean that never entered the mapping.

Every one of these would survive a grep for the behaviour's name.

## Guidance

**Judge a keeper by the expression it asserts.** Read the assertion, not the test name:

| Looks like a keeper | Actually asserts | Replace with |
|---|---|---|
| `assert!(matches!(err, Err(Variant { .. })))` | the variant | bind the fields and assert each value |
| `assert!(result.is_err())` | rejection happened | assert the code and the message fold the caller receives |
| `assert_eq!(record.status, "deleted")` after `new()` | a fresh record | drive the documented start states first |
| `assert!(spoke_allows("a","b") == false)` | a table lookup | call the mapping the caller actually uses |

**Deletion requires a state matrix, not a single success case.** For a lifecycle record, the deleted test usually covered (start state × operation × outcome). List the triples and check each against the keeper: `confirmed → delete → deleted`, `deprecated → merge → ValidationError with status unchanged`, `merged → delete → rejected, status unchanged`. One success case does not cover the matrix.

**Expect the first review to miss some.** A review seat with a file-open budget will spot-check and declare `Truncated coverage`. The uncovered portion is not approved — it needs a second bounded round. In this plan the second round found two more gaps of exactly the same class, in files the first round had listed as unaudited.

**Write the keeper table before deleting.** The table's columns are *deleted assertion · keeper path:symbol · value preserved*. Filling the third column is the work; a table with only the first two columns is a plan to lose coverage.

## Why This Matters

The failure is invisible in the usual signals: the suite stays green, the diff is pure deletion, and each keeper name reads correctly. What degrades is the ability to catch a specific regression — a swapped field, a changed status, a dropped error-code fold — which is precisely the behaviour the deleted test existed to protect.

## When to Apply

- Any plan whose deliverable is deleting tests, merging duplicate suites, or moving assertions between layers.
- Reviewing a "keeper comparison" table: check one or two entries against the assertion before trusting the rest.
- After a review round returns `Truncated coverage:` — schedule the second round rather than approving on the audited subset.
