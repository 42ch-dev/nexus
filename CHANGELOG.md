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
