---
module: STRATEGY.md (tracked root document) / agent read-write tool seam
date: 2026-09-21
problem_type: integration_issue
category: integration-issues
severity: medium
plan_id: 2026-09-21-v1.194-p3-current-architecture-docs
symptoms:
  - "A tracked document contains a line of tooling prose where content belongs — literally `[…23ln elided…]` in the middle of a table"
  - "The last line is a reader footer (`[Showing lines … Read artifact://13 for full output]`) with no trailing newline"
  - "Nothing fails: no parser, linter, link check or build reports the file, because Markdown accepts both lines as body text"
root_cause: "The file's content was written back from a paged/elided read output instead of the full file, so the tool's own elision marker replaced the 23 table rows that fell inside the elided window and the reader footer was appended at EOF. The elision marker carries no length or checksum, so the deletion is undetectable from the resulting file alone."
resolution_type: documentation_update
tags:
  - tool-output
  - write-hazard
  - elision-marker
  - reader-footer
  - tracked-doc
  - restore-from-revision
  - markdown
  - commit-hygiene
---

# Tool output written back into a tracked doc: an elision marker can delete content silently

## Problem

The repo-root tracked document `STRATEGY.md` shipped with two lines that are not document content. Committed during the v1.185 documentation work (`9db88c10e`, PR #241) and present on `HEAD` afterwards:

- a body line that was literally `[…23ln elided…]`, standing where **23 Decision Log rows (V1.92 → V1.147)** belonged;
- a last line that was literally `[Showing lines 1-105 and 129-143 of 143; 23 middle lines (21.6KB) elided. Read artifact://13 for full output]`, with no trailing newline.

Both were found while re-reading the file to retain that decision log verbatim, and both were byte-identical in a second checkout of the same revision — shipped corruption carried by the trunk, not a worktree artifact and not a local edit.

The mechanism is not "a stray line": the elided window's **content was deleted** and replaced by the tool's summary of it. The marker records a line count in prose only; there is no length, hash or structural clue in the file that says rows are missing. The `Read artifact://…` pointer in the footer is session-local, so no later reader can resolve it either.

## Symptoms

- A marker-shaped line sits where content should be (`elided`, `[Showing lines …`, `[Some lines truncated …`, `Read artifact://…`).
- The file's line count or byte count is *lower* than its history implies, with no commit message that explains a deletion.
- The affected region is large: everything inside the elided window is gone, not just a line.
- No automated check fails. A Markdown parser accepts the marker as a paragraph, the structure linter returned `OK` for both the corrupted file and the restored one, and the footer is not a link so link checking does not see it.
- The defect survives every subsequent edit that does not touch the region — it was present for many iterations.

## What Didn't Work

- **Reading the file and assuming the marker was the document's own abbreviation.** The line reads like an editorial `…` and it sits exactly where a long table would be; without a prior revision to compare against, "23 rows elided" is indistinguishable from content the document intentionally summarized.
- **Trusting the commit that carried it.** The commit's message described documentation alignment; a 23-row deletion inside a 68-line change set reads as a rewrite, and the diff's marker insertion was reviewed as prose rather than as data loss.
- **Depending on parse/build gates.** The sibling case of this mechanism — where a reader footer was appended to a `.tsx` test file — failed immediately because TypeScript cannot parse the footer. Here the corruption is valid Markdown, so nothing in the toolchain flags it, and a "it compiles/lints" argument proves nothing about content integrity.
- **Re-authoring the missing rows.** Writing replacement rows would have invented history for a decision log whose rows are supposed to be the record as written.

## Solution

Restore the region **byte-for-byte from the last revision that still had it**, and keep the later commit's intended edits:

```bash
git show 2bc1f239f:STRATEGY.md > /tmp/base.md      # last good revision of the same file
diff /tmp/base.md STRATEGY.md | grep -c '^[<>]'    # 10 → exactly the 5 intended line changes
grep -c 'elided\|artifact://' STRATEGY.md          # 0  → both markers and the footer are gone
```

The v1.185-intended edits (lines 15, 19, 22, 24, 142 relative to that revision) were re-applied on top, so the file equals the good revision **plus** the changes that commit was supposed to make. No row was authored by the repairing change. The restoration was kept in its own hunks — `@@ -106 +110,23 @@` (rows in, marker out) and `@@ -123 +161 @@` (EOF footer dropped) — separate from the task's assigned-row hunks and from the appended supersession block, so it can be reviewed or reverted independently.

The same file carried a neighbouring tracked-doc hygiene defect: a link to an ignored process path that does not resolve at `HEAD` (tracked docs must not name or link process paths). It was repaired in a follow-up commit by naming the product lock and citing the tracked authority that states it, taking the file from one unresolved relative link to zero.

## Why This Works

- **The previous revision is the only trustworthy source of the deleted content.** A byte-level diff against it proves both halves of the claim: which lines disappeared (here, exactly the marker's 23 rows) and that the change set now contains only intended edits.
- **Byte-for-byte restoration keeps the record honest.** The decision log's value is that its rows are the record as written; re-typing them from memory or from a summary is fabrication, and the diff-size check (`10` changed lines = the 5 intended edits) is what proves nothing else moved.
- **Isolating the restoration in its own hunks preserves reviewability.** A reviewer can accept the assigned-row edits, reject or rework the restoration, and see that the supersession block is separate.
- **A marker grep is a cheap, sufficient check.** The corruption shapes are a small closed set of tool phrases; one `grep` over the file settles it, and the tail/parse check covers the appended-footer variant.

## Prevention

Run this after **any** agent-assisted edit of a tracked durable document (and after any full-file write anywhere):

1. **Grep the file for marker shapes:** `elided`, `artifact://`, `[Showing lines`, `[Some lines truncated`, `[…`. A hit sitting inside document content is a defect, never a note — the only legitimate hits are documents that quote these shapes in order to describe them (as this note does).
2. **Check the tail.** The last line must be real content and the file must end with a newline; a footer or a pointer line at EOF is corruption.
3. **Diff against the previous revision and read the change set as data, not prose.** An unexplained deletion in the same change as a marker-shaped insertion is the signature. Confirm line/byte counts move the way the intent implies.
4. **Verify links resolve at `HEAD`.** Ignored process paths do not resolve for anyone else and must not be named or linked from tracked docs; if a claim needs an authority, cite a tracked one.
5. **Never write a file from a paged or elided read output.** Patch in place with targeted edits, or re-read the file in full first. If a write must re-emit the whole file, re-read it afterwards and re-run steps 1–4 — the corruption always sits at the boundary of the read window.
6. **Restore, don't re-author.** When content is missing, take it from the last good revision; record the revision in the change description.
7. **Name the tooling gap rather than assuming coverage.** No current automated gate detects this class — the structure linter passed the corrupted file — so the check is manual until something asserts "no tool markers / no process paths in tracked docs".

## When to Apply

- After any agent-assisted full-file write to a tracked document (README, `STRATEGY.md`, specs, knowledge docs, AGENTS files).
- Reviewing a documentation commit whose diff deletes a large contiguous block while inserting short prose.
- Any time a document's region "looks abbreviated" (`…`, "rows elided", "see artifact") without an author's note saying so.
- Auditing a file that was edited by an agent before any revision comparison was in place.

## Evidence

- Corrupted state: `STRATEGY.md` at `9db88c10e` (and on `HEAD` before the repair) — `[…23ln elided…]` at line 106 in place of 23 Decision Log rows, and the `[Showing lines 1-105 and 129-143 of 143; 23 middle lines (21.6KB) elided. Read artifact://13 for full output]` footer at EOF with no trailing newline; byte-identical in a second checkout, confirming a tracked defect.
- Repair: restoration commit `1a5a0dc00` on the P3 feature branch — `git show 2bc1f239f:STRATEGY.md` as the source, `diff` against it showing exactly the 5 intended line changes, `grep -c 'elided\|artifact://'` → 0, and the file's change set relative to the plan base at 23 insertions / 4 deletions with the restoration confined to `@@ -106 +110,23 @@` and `@@ -123 +161 @@`.
- Process-path sibling: the same file's unresolved link (1 → 0 unresolved relative links whole-file) repaired in the P3 fix round by naming the product lock and citing a tracked authority; the committed blob's md5 was compared against the worktree file after the commit.
- Coverage gap: the harness structure linter returned `OK` (exit 0) for the corrupted baseline, the restored file and the change report alike, so content integrity here rests on the manual checks above.
- Mechanism sibling: [reader-footer-leak-into-written-file.md](../test-failures/reader-footer-leak-into-written-file.md) — the same "content written back from a partial read" root cause on a test file, where the appended footer broke parsing immediately. This note covers the variant that deletes interior content in a tracked document and therefore fails silently.
