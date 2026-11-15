# Ask cargo instead of parsing its manifests

**Problem.** A release script needed to answer "which crates declare `publish = true`". The obvious implementation reads `Cargo.toml` and matches a regex. That implementation survived code review and passed its own tests, then took two independent QC rounds to break: a trailing comment (`publish = true # ship it`) was missed, a `publish = true` inside a `"""…"""` description was misread, a four-quote terminator made the comment mask swallow the next real declaration, and an indented `[package.metadata]` header did not end the `[package]` region. Each round fixed one corner and left another.

**Rule.** When cargo already parses a manifest, ask cargo. `cargo metadata --no-deps --format-version 1 --manifest-path <root>` returns every workspace member with `name`, an absolute `manifest_path`, and `publish` — no regex, no masking, no directory walk, and 53 fewer lines than the parser it replaced.

**Two things cargo metadata will not tell you.**

- It reports `publish: null` for a crate that omits the key *and* for one that writes `publish = true`; cargo normalizes them. So "publishable" cannot be used to mean "declares it explicitly". If the check needs to distinguish those, cargo is not the source and the check's premise needs rethinking.
- `--no-deps` lists only the workspace the manifest names. A repository with standalone crates outside that workspace needs one call per root, and the roots have to come from somewhere — a bounded list, not a recursive walk.

**The tell to act on.** Two QC rounds finding defects in the same helper, each in a different corner of a format the helper only approximates, is not a sequence of bugs. It is the approximation. Replace the data source; do not patch the corner.

**Verify.** The replacement must still fail the way the original guard did: remove one item from the input set and confirm the message names the missing item. A new data source that returns an empty list on error turns the guard green, which is worse than the parser it replaced.
