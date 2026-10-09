# Releases

Nexus ships binaries through a governed release pipeline: a dispatched version
bump opens a GitHub-signed pull request, merging that pull request creates the
annotated tag, and the pipeline attaches the three-platform `nexus-runtime`
archives plus the unsigned macOS Electron packages to a GitHub Release.

Release versions are decoupled from the internal iteration numbers: iterations
(`v1.19x`, `v1.20x`) are development milestones that never appear in a version
string or artifact name, while a release is a semver tag
`v<major>.<minor>.<patch>` — optionally carrying an `-alpha.N` / `-rc.N`
suffix (see [Versioning](#versioning)).

Two constraints shape the current flow:

- **No Apple signing yet.** Every artifact is unsigned and every desktop
  package name says so. macOS Gatekeeper and Windows SmartScreen are expected
  to warn on first launch, so every Release is published as a **prerelease**. A
  full release requires signing in place *and* an explicit dispatch opt-out —
  see [Signing arrival](#signing-arrival).
- **One build truth.** The pipeline does not rebuild anything. It calls the
  producers that already serve manual and push-triggered builds
  (`runtime-build.yml`, `desktop-electron-build.yml`) through `workflow_call`,
  so a released artifact is the producer's own artifact rather than a second
  build that could drift from it.

## Versioning

- A release version is `X.Y.Z`, `X.Y.Z-alpha.N` or `X.Y.Z-rc.N`: three
  canonical numeric components (no leading zeros — `01.2.3` is rejected) with
  an optional lowercase `alpha`/`rc` suffix whose `N` starts at 1 (`alpha.0`
  and `alpha.01` are rejected). Build metadata (`+…`), any other label and any
  case variant are rejected; versions are never normalised. Components are
  compared without number conversion so very long fields stay distinct.
- `alpha` marks an explicitly unstable daily build; `rc` a release candidate
  put in front of testers before the stable of the same core. The conventional
  ladder within one core is `X.Y.Z-alpha.N` → `X.Y.Z-alpha.(N+1)` →
  `X.Y.Z-rc.1` → `X.Y.Z-rc.(N+1)` → `X.Y.Z`; a core increment starts a new wave
  at `<next-core>-alpha.1` (or `X.Y.(Z+1)-alpha.1` for a patch wave). Skipping
  rungs is allowed — every rung transition is strictly greater under the
  ordering below, so the tooling accepts it without special cases.
- Ordering follows SemVer §11 as a single rule set: the core decides first
  (`0.1.9 < 0.2.0-alpha.1`), and within an equal core a suffixed version is
  less than the clean one (`0.2.0-alpha.1 < 0.2.0-alpha.2 < 0.2.0-rc.1 <
  0.2.0`). Dispatch validation and the bump accept any strictly-greater version
  and refuse an equal or lower one.
- The tag carries the full version including any suffix
  (`v0.2.0-alpha.1`, `v0.2.0-rc.1`, `v0.2.0`), so the tag and the download page
  read honestly; `v0.2.0-rc.1` and the later `v0.2.0` are distinct refs.
- Each release of any channel covers the range since the previous release:
  `generate-changelog.mjs` takes the newest `v*` tag as its base, re-derived
  with the ordering above (never git's version sort, which orders suffixed tags
  incorrectly), falling back to the commit that introduced `CHANGELOG.md` for
  the first release.
- While Nexus is pre-1.0: MINOR for a feature increment, PATCH for fixes only.
- `0.0.0` is the never-released workspace baseline (no release has ever
  shipped). The first governed release is **`0.1.0`**.
- The version SSOT has ten files, all bumped by one commit: root `Cargo.toml`
  (`[workspace.package].version`, inherited by every workspace crate), root
  `package.json`, `apps/desktop-electron/package.json`,
  `apps/desktop-electron/resources/product.json`, the native loader
  `packages/nexus-native/package.json` (its `@42ch/nexus-native-*` pins use the
  version-stable `workspace:*` protocol, which `pnpm pack` replaces with the
  exact platform version), the four `packages/nexus-native-<platform>/package.json`
  manifests, and the regenerated `Cargo.lock`. The lockfile is written by Cargo
  (`cargo update --workspace --offline`), not by a text edit; the Electron
  packaging preflight asserts that the root, product and Electron versions
  agree; and the native loader fences each platform manifest against the built
  artifact's embedded version before the database is opened.
- Dispatch validation refuses a version that is not strictly greater than the
  root `package.json` version on `origin/main`, and refuses a version whose
  `v<version>` tag already exists.

## Dispatch a release

`New release` (`.github/workflows/new-release.yml`) is the pipeline's only
entry point and is dispatched from `main`. Nothing is tagged and no Release is
published until the pull request it opens is merged into `main`.

### Inputs

| Input | Type | Default | Meaning |
|-------|------|---------|---------|
| `version` | string, required | — | Release SemVer `X.Y.Z`, `X.Y.Z-alpha.N` or `X.Y.Z-rc.N`, strictly greater than `origin/main`. Recommended first dispatch: `0.1.0`. |
| `summary` | string, optional | `""` | Human-written framing. It is prepended to the generated changelog in the bump pull request, in the tag annotation and in the Release body — use it to describe the pre-baseline history on the first release. |
| `prerelease` | boolean, optional | `true` | Requested prerelease marking. While signing is unimplemented the pipeline forces `true` regardless of this value ([below](#prerelease-gate-and-the-nexus-prerelease-trailer)). |

### Walkthrough

1. **Clear the one-time repository prerequisite (admin).** Repository
   *Settings → Actions → General* must allow GitHub Actions to create pull
   requests. Without it the bump-PR step cannot open the pull request.
2. **Pre-check the version locally (optional, read-only).** With `origin/main`
   fetched, the same assertion the pipeline runs can be executed by hand:

   ```sh
   git fetch origin main
   node tooling/release/assert-version-greater.mjs 0.1.0 --base-ref main
   # Version OK: 0.1.0 > 0.0.0 (origin/main); v0.1.0 does not exist
   ```

3. **Dispatch.** Actions → *New release* → *Run workflow* on `main` with the
   inputs above. Runs share the requested-version concurrency group with
   `cancel-in-progress: false`: a running run is not cancelled, but Actions keeps
   only one pending run per group; a newer pending dispatch replaces the older
   pending one. This is serialization, not a FIFO queue, so do not rely on every
   repeated dispatch executing.
4. **Preflight.** `Warm Cargo registry cache` runs `cargo fetch --locked`
   **before** `Run release tooling tests` (`node --test
   'tooling/release/*.test.mjs'`, Node 22 or newer — the quoted glob needs a
   Node release that expands test-file globs). The cache is needed by the tests'
   real offline Cargo lockfile regeneration path as well as the later bump.
5. **Validate.** `Validate SemVer, greater-than-main, and unused tag`
   re-runs `assert-version-greater.mjs` against a full-history checkout.
6. **Open the signed bump pull request.** In the `create-release-pr` job,
   `Fetch main, revalidate, and create release branch` re-checks the version
   and starts `release/<version>` from `origin/main`. `Bump version surfaces
   and lockfile` then runs `cargo fetch --locked`,
   `node tooling/release/bump-version.mjs <version>` (the ten version
   manifests — the five hand-written lockstep files plus the five native npm
   files — then `cargo update --workspace --offline` for `Cargo.lock`) and a
   confirming `cargo update --workspace --offline`. `Write dispatch summary`
   stages the
   `summary` input, and `Generate and prepend CHANGELOG entry` prepends the
   generated section to `CHANGELOG.md`. `Push GitHub-signed release commit`
   creates the single commit `chore(release): bump version to <version>` —
   GitHub-signed through the GraphQL `createCommitOnBranch` API, so it stays
   valid when signature-protected branches arrive — with a
   `Nexus-Prerelease: <toggle>` trailer. The replacement commit is parented on
   the existing `release/<version>` head and published with an expected-head
   compare-and-swap (CAS); on first creation, the branch starts from its base
   ref. A moved head makes the helper refuse without overwriting it. `Ensure
   release label exists` and `Open or update release pull request` finish the
   job by opening or updating a pull request titled
   `chore(release): bump version to <version>`, labeled `release`, whose body is
   the `summary` followed by the new changelog section.
7. **Review, then merge with a merge commit.** Review the diff (the version
   files, `Cargo.lock`, `CHANGELOG.md`) and the generated body. Merge the pull
   request with a **merge commit**: the prerelease trailer is read from the
   merge commit's second parent (the pull request head commit), so a squash or
   rebase merge has no second parent and the tag job fails closed — nothing is
   tagged and no Release is published.

   To correct the summary or the generated entry before merging, dispatch the
   same version again; the replacement commit parents on the current release
   branch head (CAS), and the pull request is updated in place. Do not hand-edit
   the release branch: a hand-made commit is not GitHub-signed.

## Pipeline stages

Merging the `release`-labeled pull request into `main` starts
`Release` (`.github/workflows/release.yml`):

| Stage | Job | Gate | What it does |
|-------|-----|------|--------------|
| Tag | `tag` | merged PR carrying the `release` label, or a `v*` tag push | `Ensure annotated tag and resolve prerelease`: resolves the version from `package.json` at the merge commit, creates and pushes the annotated tag `v<version>` at that commit if it is missing (annotation = the first non-empty line in the bump commit body: the summary's first line when provided, otherwise the `Nexus-Prerelease:` trailer), requires an existing tag to be annotated and to point at the same commit, reads the `Nexus-Prerelease` trailer from the merge commit's second parent, and resolves the effective prerelease value. |
| Verify | `verify-version` | `tag` succeeded | `Assert lockstep version`: checks out `refs/tags/v<version>` and asserts the five hand-written version files are equal (`Cargo.toml`, root `package.json`, `apps/desktop-electron/package.json`, `apps/desktop-electron/resources/product.json`, and `apps/nexus-service/package.json`; `assert-lockstep-version.mjs`). |
| Producers | `runtime-build` | `tag` and `verify-version` succeeded | Reusable call into `runtime-build.yml` (job `runtime-build`): the three-platform matrix (`windows-x64`, `macos-arm64`, `linux-x64`) builds `nexus-runtime-<os>-<arch>.zip` plus a `.sha256` sidecar, and smoke-tests `--version` on each runner. |
| Producers | `desktop-electron-build` | `tag` and `verify-version` succeeded | Reusable call into `desktop-electron-build.yml` (job `package`): the two macOS legs (darwin arm64, darwin x64) package `Nexus-<version>-darwin-<arch>-unsigned.dmg`, `Nexus-<version>-darwin-<arch>-unsigned.app.zip`, `receipt.json` and `SHA256SUMS`, with signing-dispatch sentinels proving no codesign/notarize call ran. |
| Publish | `publish` | all four jobs succeeded | `Download runtime artifacts` and `Download desktop artifacts` collect the five artifact sets; `Assemble release notes and assets` renders the Release body (changelog section plus the fixed footer) and stages the complete 14-asset set; `Stage, upload, and publish GitHub Release` creates or reconciles the Release as a draft with the effective prerelease value, uploads all assets with `--clobber`, and clears `draft` only after every upload succeeds; on failure an existing Release that the retry returned to draft is restored to the draft/prerelease state it had on entry while no asset has been replaced, and is deliberately left unpublished — never restored to public — once any asset replacement has begun, so a partial old/new asset set is never downloadable. |

Permissions stay least-privilege: `new-release.yml` requests
`contents: write` + `pull-requests: write`, the release workflow grants
`contents: write` only to the `tag` and `publish` jobs, and no job requests
`id-token` (nothing is published to a registry).

The published Release carries:

| Asset | Source |
|-------|--------|
| `nexus-runtime-windows-x64.zip`, `nexus-runtime-macos-arm64.zip`, `nexus-runtime-linux-x64.zip` | `runtime-build` tiers, unchanged |
| The same three `.zip.sha256` sidecars | `runtime-build`, unchanged |
| `Nexus-<version>-darwin-arm64-unsigned.dmg`, `Nexus-<version>-darwin-x64-unsigned.dmg`, `Nexus-<version>-darwin-arm64-unsigned.app.zip`, `Nexus-<version>-darwin-x64-unsigned.app.zip` | `desktop-electron-build`, unchanged (no Windows Electron packaging exists; Windows desktop users are served by the `nexus-runtime` windows-x64 zip) |
| `Nexus-<version>-darwin-arm64-SHA256SUMS`, `Nexus-<version>-darwin-x64-SHA256SUMS`, `Nexus-<version>-darwin-arm64-receipt.json`, `Nexus-<version>-darwin-x64-receipt.json` | byte-identical copies of each architecture's `SHA256SUMS` and `receipt.json`, published under architecture-specific names so the two legs cannot overwrite each other |

## Prerelease gate and the `Nexus-Prerelease` trailer

`tooling/release/release-config.mjs` exports `signingImplemented`, the single
switch asserting whether Apple-signed artifacts exist yet; it is `false` today.
The effective prerelease value of a Release is:

```text
effective prerelease = hasSuffix(version) OR dispatch toggle OR NOT signingImplemented
```

so a full (non-prerelease) Release requires a clean (unsuffixed) version
**and** an explicit `prerelease: false` dispatch **and**
`signingImplemented === true`. A version carrying an `-alpha.N` / `-rc.N`
suffix is always published as a prerelease: dispatching one with
`prerelease: false` is accepted but overridden, and that holds even after
signing lands. While the export is `false`, every Release is a prerelease no
matter what the dispatch asked for.

The dispatch toggle travels with the bump commit as a `Nexus-Prerelease:
true|false` trailer, appended by the signed-commit script and read by the `tag`
job from the merge commit's second parent
(`git show -s --format=%B "$MERGE_SHA^2"`). An absent, duplicated or unreadable
trailer fails closed to prerelease. You can check the trailer before merging:

```sh
node tooling/release/effective-prerelease.mjs --message-file <(git show -s --format=%B <head-sha>) --version 0.2.0-rc.1
node tooling/release/effective-prerelease.mjs --toggle false   # prints true today
node tooling/release/effective-prerelease.mjs --version 0.2.0-rc.1   # forced prerelease
```

The `tag` job passes the resolved version through `--version "$version"`, so
both entry paths (merge and tag push) evaluate the suffix. The tag is
`v<version>` including any `-alpha.N` / `-rc.N` suffix, so a
suffixed (always-prerelease) Release never wears a clean-looking tag. The
workflow creates a draft with `--draft`, adding
`--prerelease` when the effective value is true; an existing Release is first
reconciled to draft state and the same prerelease value. Only after every asset
upload succeeds does the workflow clear `draft`, retaining the effective
prerelease value. Because an existing Release may already be public, the step
records its draft/prerelease state on entry and handles the failure path (an
`EXIT` trap) with two outcomes. If no asset has been replaced yet, visibility is
preserved: the recorded draft/prerelease state is best-effort restored, so a
failed retry leaves a published Release public. Once at least one `--clobber`
upload has succeeded, the original assets are gone and the release is instead
left unpublished — draft — with a loud diagnostic naming the partial state and
the recovery action, so no download page can serve a mixed old/new set whose
checksums disagree with the release notes; re-run the workflow to complete the
replacement.

## Release body and footer

The publish job's `Assemble release notes and assets` step builds the body in
two parts: the CHANGELOG section for the version (the same text the bump PR
added — human `summary` plus the conventional-commit groups Features / Fixes /
Docs & chores), then the fixed footer below. The section is extracted by
matching the generator's own header form `## [<version>] - ` and stopping only
at the next such version header, so a `summary` that itself contains a `## `
Markdown heading is preserved instead of truncating the notes; the bump PR body
uses the identical extraction. Checksums are read from the
producers' sidecars — the three `.zip.sha256` files and each architecture's
`SHA256SUMS` — and are never recomputed; a missing sidecar or a missing line for
one of the seven distributed artifacts fails the publish job.

The literal footer template (`<digest>` is the 64-character lowercase hex digest
read from the producer sidecar, `<version>` the released version):

~~~text
Every artifact in this release is unsigned. macOS Gatekeeper and Windows SmartScreen are expected to warn on first launch. This release is a prerelease because signing is not implemented. Signed packages will replace these artifacts without changing how they are installed once signing lands.

## SHA-256 checksums
```text
<digest>  nexus-runtime-windows-x64.zip
<digest>  nexus-runtime-macos-arm64.zip
<digest>  nexus-runtime-linux-x64.zip
<digest>  Nexus-<version>-darwin-arm64-unsigned.dmg
<digest>  Nexus-<version>-darwin-arm64-unsigned.app.zip
<digest>  Nexus-<version>-darwin-x64-unsigned.dmg
<digest>  Nexus-<version>-darwin-x64-unsigned.app.zip

Desktop producer sidecars are uploaded unchanged as architecture-specific assets:
Nexus-<version>-darwin-arm64-SHA256SUMS
Nexus-<version>-darwin-arm64-receipt.json
Nexus-<version>-darwin-x64-SHA256SUMS
Nexus-<version>-darwin-x64-receipt.json
```

## Installation
- **nexus-runtime-windows-x64.zip**: Unzip and run `nexus-runtime.exe` directly. Expect a SmartScreen warning on first run because it is unsigned.
- **nexus-runtime-macos-arm64.zip**: Unzip, run `chmod +x nexus-runtime`, then launch it. Gatekeeper may block the unsigned binary on first launch.
- **nexus-runtime-linux-x64.zip**: Unzip, run `chmod +x nexus-runtime`, then launch it.
- **Nexus-<version>-darwin-<arch>-unsigned.dmg** (arm64 and x64): Open the DMG and drag `Nexus` to Applications. On first launch, right-click → Open to bypass Gatekeeper for the unsigned app.
- **Nexus-<version>-darwin-<arch>-unsigned.app.zip** (arm64 and x64): Unzip and run `Nexus.app`; on first launch, use right-click → Open as above.
~~~

The wiring is already in place for signed packages: signing arrives by
replacing unsigned artifacts in the same seven slots, and the install steps stay
the same.

## Verifying the pipeline without shipping a release

Every check below is read-only or runs in a scratch copy; none of them
dispatches a producer, pushes a tag, or creates a Release.

1. **Parse the workflows and syntax-check their scripts.** No network, no
   runner:

   ```sh
   ruby -ryaml -rtmpdir -rfileutils -e '
     %w[release new-release].each do |name|
       doc = YAML.load_file(".github/workflows/#{name}.yml")
       doc["jobs"].each do |job, spec|
         Array(spec["steps"]).each_with_index do |step, i|
           next unless step["run"]
           path = "#{Dir.tmpdir}/#{name}-#{job}-#{i}.sh"
           File.write(path, step["run"])
           system("bash", "-n", path) or abort("bash -n failed: #{path}")
         end
       end
     end
     puts "workflow YAML parses; every run block passes bash -n"'
   ```

2. **Run the tooling tests** — the same command as the dispatch preflight, and
   the reason the dispatch runs it before touching anything:

   ```sh
   node --test 'tooling/release/*.test.mjs'
   ```

3. **Dry-run the dispatch decisions** (read-only against the live repository):

   ```sh
   git fetch origin main
   node tooling/release/assert-version-greater.mjs 0.1.0 --base-ref main
   node tooling/release/generate-changelog.mjs --version 0.1.0    # stdout; stderr names the resolved base
   node tooling/release/effective-prerelease.mjs --toggle false   # forced prerelease today
   ```

   `generate-changelog.mjs` writes nothing unless `--prepend` is passed, and its
   stderr line states whether the range came from the previous release tag of
   any channel (the semver-maximum `v*` tag, never git's version sort) or from
   the first-release fallback.

4. **Run the bump in a scratch checkout**, never in a worktree you intend to
   keep clean. The bump needs the Rust toolchain plus a populated registry cache
   (its `cargo update --workspace --offline` never fetches):

   ```sh
   scratch="$(mktemp -d)"
   git clone --quiet --local . "$scratch/nexus"
   cd "$scratch/nexus"
   cargo fetch --locked
   NEXUS_REPO_ROOT="$scratch/nexus" node tooling/release/bump-version.mjs 0.1.0
   NEXUS_REPO_ROOT="$scratch/nexus" node tooling/release/assert-lockstep-version.mjs
   git diff --stat     # ten version manifests + Cargo.lock; nothing is committed
   ```

5. **Render the Release body locally.** The footer is composed by an ordinary
   shell step, so it can be exercised against synthetic artifact directories —
   the only way to see the exact published body without running the pipeline:

   ```sh
   work="$(mktemp -d)"; cd "$work"
   mkdir -p runner/release-artifacts
   for slug in windows-x64 macos-arm64 linux-x64; do
     d="runner/release-artifacts/nexus-runtime-$slug"; mkdir -p "$d"
     printf 'fake' > "$d/nexus-runtime-$slug.zip"
     printf '%s  nexus-runtime-%s.zip\n' "$(printf '1%.0s' {1..64})" "$slug" > "$d/nexus-runtime-$slug.zip.sha256"
   done
   for arch in arm64 x64; do
     d="runner/release-artifacts/nexus-desktop-unsigned-darwin-$arch"; mkdir -p "$d"
     printf 'fake' > "$d/Nexus-0.1.0-darwin-$arch-unsigned.dmg"
     printf 'fake' > "$d/Nexus-0.1.0-darwin-$arch-unsigned.app.zip"
     printf '{}' > "$d/receipt.json"
     printf '%s  Nexus-0.1.0-darwin-%s-unsigned.dmg\n%s  Nexus-0.1.0-darwin-%s-unsigned.app.zip\n' \
       "$(printf '2%.0s' {1..64})" "$arch" "$(printf '3%.0s' {1..64})" "$arch" > "$d/SHA256SUMS"
   done
   printf '# Changelog\n\n## [0.1.0] - 2026-10-08\n\n### Features\n- **release**: example (abc1234)\n' > CHANGELOG.md
   : > runner/env
   ruby -ryaml -e 'step = YAML.load_file("<repo>/.github/workflows/release.yml")["jobs"]["publish"]["steps"].find { |s| s["name"] == "Assemble release notes and assets" }; File.write("assemble.sh", step["run"])'
   VERSION=0.1.0 RUNNER_TEMP="$work/runner" GITHUB_ENV="$work/runner/env" bash assemble.sh
   cat runner/release-body.md
   ```

   Substituting a real repository path for `<repo>`, this renders: the changelog
   section, the unsigned disclosure, a `## SHA-256 checksums` block with one line
   per distributed artifact, the sidecar note, and the `## Installation` hints.

6. **Know what is not a dry run.** Dispatching either producer
   (`workflow_dispatch` on `runtime-build.yml` / `desktop-electron-build.yml`)
   performs real builds; merging any `release`-labeled PR and pushing a `v*` tag
   both enter the release workflow for real. The only GitHub-side rehearsal is a
   dispatch of `New release` with a throwaway version, which creates a real
   `release/<version>` branch and PR — close the PR and delete the branch
   afterwards. Nothing is tagged or published until a `release`-labeled PR is
   merged into `main`.

## Recovery

The release workflow can be re-entered after failure. Publication stages the
Release as a draft, updates notes and the complete asset set, and publishes only
after every upload succeeds. A failed update remains unpublished for recovery.

- **Re-run the run.** Actions → *Release* → the failed run → *Re-run failed
  jobs* replays the original event with the same payload. The `tag` job
  re-creates the tag only if it is missing, then verify → producers → publish
  runs again. Existing Releases are explicitly returned to draft state before
  asset updates; successful completion clears draft and publishes them. The step
  records the Release's draft/prerelease state before that and restores it
  (best-effort, via an `EXIT` trap) if a later step fails **before any asset has
  been replaced**, so a retry that started from a published Release returns it to
  public. A failure **after** a successful `--clobber` upload cannot restore the
  original assets, so the release is deliberately left unpublished instead, with
  an `::error::` diagnostic telling the operator to re-run to finish the
  replacement — a partial old/new set is never exposed on a public download page.
- **Tag-push re-entry.** `push: tags: ['v*']` enters the same workflow: the
  `tag` job requires the pushed tag to be annotated, takes the tagged commit as
  the merge commit, and carries it through verify → producers → publish. This
  path fires when a maintainer pushes a `v*` tag (for example, after a failure
  that never created the tag).

  The pipeline's own tag push uses the default `GITHUB_TOKEN`, and GitHub does
  not start workflow runs for events created by that token, so a normal merge
  never re-enters through this path — it is an operator tool, not a loop.

  To reach it for an already-pushed tag you must delete the remote tag and push
  it again. GitHub turns a Release whose tag is deleted into a draft. It stays
  unpublished if re-entry fails; on success, the workflow uploads the complete
  set and publishes it. Prefer re-running the failed run when possible.

- **Existing-tag guards.** When the tag already exists, the `tag` job requires
  it to be annotated and to point at the merge commit; a lightweight tag, or one
  pointing somewhere else, fails closed. A failure in either producer prevents
  `publish`. During `publish`, both new and existing Releases are kept draft
  while notes and assets are updated; all uploads must succeed before the Release
  becomes public. The workflow implements GitHub Releases API draft behavior by
  setting `draft=true` before uploads and `draft=false` afterwards. On failure,
  an existing Release that a retry returned to draft is restored to its entry
  state only while no asset has been replaced; once a replacement has begun it
  stays unpublished (with a diagnostic) rather than exposing a partial set.

## Signing arrival

Signing changes exactly one switch, and the flow around it stays as documented:

1. Set `signingImplemented` to `true` in `tooling/release/release-config.mjs`.
   The prerelease gate stops forcing prereleases.
2. Ship signed packages from the desktop producer in the same seven artifact
   slots. The `-unsigned` suffix and the disclosure paragraph describe the
   current unsigned state; when signed packages replace them, update the publish
   job's filename patterns and the footer text in `release.yml` to match (this
   page documents the unsigned text it currently renders).
3. Dispatch with `prerelease: false` to publish a full release. A version
   carrying an `-alpha.N` / `-rc.N` suffix still publishes as a prerelease
   regardless of the toggle. Dispatch inputs, the signed bump PR, the annotated
   tag, lockstep verification, the producer calls and the publish step are
   unchanged.
