# Changelog

Nexus release notes, one section per governed release. Sections are generated
from this repository's conventional commits by
`tooling/release/generate-changelog.mjs`: `new-release` prepends the next
section in the version-bump pull request, and the same text becomes the GitHub
Release body (the surrounding process is documented in
[docs/release.md](docs/release.md)).

### Section format

- Heading: `## [<version>] - <YYYY-MM-DD>` — the clean release version. A
  prerelease is GitHub Release metadata, never part of a version or a tag.
- Groups, in order: **Features** (`feat`), **Fixes** (`fix`), **Docs & chores**
  (`build`, `chore`, `ci`, `docs`, `perf`, `refactor`, `revert`, `style`,
  `test`). Entries read `- **scope**: description (abcdef0)`; subjects that are
  not conventional commits are skipped.
- `chore(release): bump version to <version>` commits are excluded: the notes
  never list the release machinery itself.

### Release range

- **First governed release.** While no `v*` tag exists, the range starts at the
  commit that introduced this file:

  ```sh
  git log --diff-filter=A -1 --format=%H -- CHANGELOG.md
  ```

  Everything merged after the changelog mechanism landed is in range; the
  dispatcher's `summary` input frames the earlier, pre-baseline history, which
  is intentionally not backfilled.
- **Subsequent releases.** The newest `v*` tag
  (`git tag --list 'v*' --sort=-v:refname | head -1`) to the merge commit of
  the version-bump pull request.

<!-- Maintenance: keep `## ` headings out of this preamble. The release tooling
     inserts each new section above the first `## ` line and extracts a section
     by matching `## [<version>] - `. `### ` headings are safe. -->

## [0.1.0] - 2026-10-08

First governed release — Nexus 0.1.0.

Nexus is a local-first narrative-orchestration platform: the direct-core nexus42 CLI, the web/Electron reference creator surfaces, the standalone TypeScript service, and the independent nexus-runtime Connect host, on the Rust-core/TypeScript-services boundary with the Actor product model and the Harness / Canvas / Computable pillars.

Every artifact is unsigned; macOS Gatekeeper and Windows SmartScreen warn on first launch, and this release is published as a prerelease until Apple signing lands.

### Fixes
- **release**: keep the native platform pins version-stable via workspace:* (646eaf1)
- **release**: include the native npm manifests in the version surface set (e96f376)

### Docs & chores
- **nexus-native**: derive the platform manifest version in loader-negative (ff00936)
- **nexus42**: derive the --version assertion from CARGO_PKG_VERSION (4e1b87c)
- bump react-flow skill submodule to a224eeb (correctness fixes) (e5d3fb5)
- **deps**: npm minor-patch group (14) + msw 3.0.0 adoption — supersedes #360 #361 (#365) (2d6ac67)
- **deps**: bump the cargo-minor-patch group across 1 directory with 5 updates (#363) (13bea60)
- **deps**: bump actions/setup-node from 4.4.0 to 7.0.0 (#357) (ee999ff)
- **deps**: bump .agents/skills/react-flow from `a224eeb` to `b060a63` (#356) (382b51f)
- **deps**: bump codex-codes from 0.156.1 to 0.158.0 (#359) (421c794)
