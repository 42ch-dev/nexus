---
module: .github/workflows/release.yml
date: 2026-09-30
problem_type: integration_issue
category: integration-issues
severity: medium
plan_id: 2026-09-30-v1.202-p3-release-governance
symptoms:
  - "Both architectures' sidecars appear in the Release body under distinct names, yet only one SHA256SUMS and one receipt.json asset exist on the Release"
  - "A re-run shows x64 replacing arm64 sidecars despite the #arm64 label suffixes on the upload arguments"
  - "The fix round also double-queued assets: originals and staged copies both uploaded"
root_cause: "gh release upload <file>#<label> sets the asset's DISPLAY LABEL, not its asset name; GitHub Release assets are keyed by basename, so identically named sidecars from two architectures collide under --clobber. Compounding: the validation helper appended its input path to the upload queue, re-introducing original basenames even after correctly staged copies existed."
resolution_type: code_fix
tags:
  - gh-cli
  - release-assets
  - asset-naming
  - sidecars
  - clobber
  - staging-copies
---

# `gh release upload <file>#<label>` sets a display label, not the asset name

## Problem

The v1.202 P3 publish job must attach two architectures' checksum sidecars, but both producers emit identically named files (`SHA256SUMS`, `receipt.json`). GitHub Release assets are keyed by **basename**, so the second architecture's upload replaces the first (`--clobber`). The first fix appended `#Nexus-<version>-darwin-arm64-SHA256SUMS` to the upload path — and the collision persisted.

## Symptoms

- Both architectures' sidecars appear in the Release body under distinct names, yet only one `SHA256SUMS` and one `receipt.json` asset exist on the Release.
- A re-run shows x64 replacing arm64 sidecars despite the `#arm64` label suffixes.
- The fix round also double-queued: originals and staged copies both uploaded.

## What Didn't Work

- `gh release upload <local-path>#<label>` — per the [gh CLI contract](https://cli.github.com/manual/gh_release_upload), the `#` suffix sets the asset's **display label**; the asset name remains the local file's **basename**. Labels cannot deduplicate same-basename assets.
- Keeping the originals in the upload queue "for validation" — helpers that append their input to the queue re-introduce the colliding basenames even after correct staged copies exist.

## Solution

Stage byte-identical copies under the desired architecture-specific basenames, validate the originals without queueing them, and upload only the staged copies once each:

```bash
# validate only (no queueing)
[[ -f "$arch/SHA256SUMS" ]] || exit 1
# stage under the advertised name (bytes unchanged)
cp "$arch/SHA256SUMS" "Nexus-${VERSION}-darwin-${arch}-SHA256SUMS"
# queue the staged name only
files+=("Nexus-${VERSION}-darwin-${arch}-SHA256SUMS")
```

Then upload without `#` suffixes and verify the remote asset set is exactly the staged names before publishing (draft-first keeps a partial set invisible while uploading).

## Why This Works

GitHub keys Release assets by filename; renaming the local file is the only way to control the asset name through the upload path. Byte-identical copies preserve the producers' checksum artifacts verbatim (the release body reads digests from these files — never recompute), and validating-without-queueing removes the second collision source.

## Prevention

- Any multi-architecture asset upload: inventory the producers' **output basenames** first; if any collide, plan a staging-rename step before the upload loop.
- Assert queue invariants in a local dry-run: final queue contains no duplicate basenames and exactly the advertised asset set (the v1.202 fix round shipped a Ruby scratch harness doing exactly this).
- Never treat `#label` as part of the asset name in docs or release bodies; advertise the staged basename.

## Related

- `gh release upload` manual: <https://cli.github.com/manual/gh_release_upload>
- Fixed in `1f7a5fb38` (queue) + `9ee32d5aa` (draft-first upload/verify/publish), v1.202 P3; review history in `.mstar/sdd/2026-09-30-v1.202-p3-release-governance/review/qc3.md` (QC3-003 + revalidation round 1).
