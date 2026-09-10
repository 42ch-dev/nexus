---
module: packages/nexus-ui
date: 2026-09-10
problem_type: test_failure
category: test-failures
severity: low
symptoms:
  - "packages/nexus-ui/src/components/transport-error-block.test.tsx ended with a tooling footer line instead of the test file's closing content"
  - "The committed file no longer parsed; the suite could not run until the stray line was removed"
  - "The line had no trailing newline and appeared only in one fix-round commit"
root_cause: "The file was re-written from a paginated read output and the reader's footer marker ([Showing lines … Use :… to continue]) was copied into the written content."
resolution_type: code_fix
tags:
  - write-hazard
  - tool-output
  - reader-footer
  - test-file
  - commit-hygiene
---

# A reader/tool footer leaked into a committed source file

## Problem

One v1.187 fix-round commit re-wrote `packages/nexus-ui/src/components/transport-error-block.test.tsx` and appended a reader footer line:

```
[Showing lines 1-300 of 329. Use :301 to continue]
```

The file (329 lines) had been read in a page-limited view; the footer that the reader appends to that view was included when the content was written back. Parsing broke; the stray line shipped in the feature branch until review caught it, and a dedicated follow-up commit deleted it with no assertion or behavior change.

## Symptoms

- A line that looks like tooling metadata — not source — is the last line of the file, typically with no trailing newline.
- The change was made by an agent-assisted fix round whose edit re-assembled a large file rather than patching it in place.
- The file type-checks/parses nowhere: the failure is immediate on any parse, so it never reaches a test run.

## What Didn't Work

- **Assuming the writer's intent was correct because the diff was small.** The extra line was the only added line in that hunk and read as plausible trailing content at a glance; only an actual parse/read of the file end exposed it.
- **Relying on the author's self-check.** The implementing agent reported the file as done; the defect survived until the diff was reviewed line-by-line at the file boundary.

## Solution

Remove the footer line in a dedicated commit:

```diff
   });
 });
 
-[Showing lines 1-300 of 329. Use :301 to continue]
\ No newline at end of file
```

After the repair, the whole plan diff was swept for other reader/report markers and tool prose; the verify pass confirmed the file ends at its normal test closing and that no other changed file carries injected marker text.

## Why This Works

The line is pure tooling output with no meaning in the source; deleting it restores the original parse. The important half of the fix is the **sweep**: because the corruption mechanism is "content was reassembled from a partial view", a single repaired file does not prove the same leak did not happen in a sibling file from the same edit round.

## Prevention

- Never re-assemble a file's full content from a paginated read; patch in place (targeted edits) or write the file from an authoritative source.
- After any full-file write, re-read the file's tail and check the last lines parse as source — the corruption is always at the boundary of the read window.
- Reviewers: scan the changed files for tool-marker shapes (`[Showing lines …]`, `[Some lines truncated …]`, artifact/report prose) before approving a fix-round commit; a marker line at EOF is a defect, not a comment.
- Keep a fix commit focused: the repair here was a one-line commit whose message says explicitly that no assertion or behavior changed, which keeps the evidence trail clean.
