---
module: apps/design-studio
date: 2026-09-10
problem_type: ui_bug
category: ui-bugs
severity: medium
symptoms:
  - "Reset comparison cleared the frames' ready/failed labels but the frame documents stayed mounted; fixture state was not reset"
  - "Retry after a failed frame could not recover the frame — the failed document was never replaced"
  - "A marker set on the frame window before Reset was still present afterwards"
root_cause: "Reset/Retry only cleared component state and bumped a counter that recomputed the frame src; the computed URL is identical for the same path/hash/theme and the <iframe> had no React key, so React reused the same element and the browser never created a new document."
resolution_type: code_fix
tags:
  - design-studio
  - iframe
  - pair-view
  - react-key
  - remount
  - retry
---

# Iframe reset/retry must remount by key, not just recompute the URL

## Problem

The pair view contract requires **Reset comparison** and **Retry** to remount both frame documents (fresh fixture state; a timed-out or errored frame gets a new document). The first implementation reset state and recomputed each frame's `src` through a reset counter, which looked correct in code review but changed nothing in the DOM.

## Symptoms

- After Reset, a marker installed on the frame's `contentWindow` was still there — proof the document had never been replaced.
- Fixture interactions inside the frames survived Reset, so "reset" did not return the comparison to its initial state.
- A frame that had hit the 10-second ready deadline stayed broken through Retry: the failed document was reused, so the new deadline expired against the same stale document.

## What Didn't Work

- **Bumping a memo dependency that rebuilds the same URL.** `buildStudioEmbedSrc(path, hash, theme)` is a pure function of data that does not change on reset; the recomputed string was byte-identical, so React did not touch the attribute and the browser did not navigate or reload.
- **Clearing ready/failed state as the reset.** State reset only changes the parent's bookkeeping (overlay copy, timeout scheduling) — the frame document and every interaction inside it are untouched.
- **Relying on fragment navigation.** Deep-link/hash updates inside an already-loaded document are same-document navigations; they cannot reset fixture state either.

## Solution

Key each frame element on the effective reset counter so React destroys and recreates the `<iframe>`:

```tsx
const effectiveResetKey = resetKey + localResetKey; // parent reset + local Retry
// …
<iframe key={effectiveResetKey} ref={iframeRef} src={src} title={title} />
```

`resetFrames()` clears the ready/failed state and timers **and** increments the local key; the parent's Reset button increments its own counter; the readiness effect re-runs (deadline re-armed) because the reset key is part of its dependency list.

## Why This Works

A changed React `key` makes React unmount the old element and mount a new one, so the browser tears down the old iframe document and starts a fresh navigation. The URL string can stay identical — the remount, not the URL, is the reset mechanism. Re-keying also re-arms the readiness deadline, which is what makes Retry meaningful for a frame that previously timed out.

## Prevention

- For any embedded document, treat "reset" as **remount**, and prove it with an observable marker on the frame window (`window.__marker = …` before reset, assert it is gone after) rather than with a screenshot.
- Review check: a reset path that only sets state/`src` and cannot change the element's identity is not a reset.
- Keep the remount counter in the frame container component; both Reset and Retry must route through the same `resetFrames()`.
- Related: [iframe-pair-view-theme-isolation.md](../architecture-patterns/iframe-pair-view-theme-isolation.md) (the surrounding pair-view contract and its readiness handshake).
