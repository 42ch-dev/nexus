# Cross-writer config lock protocol (daemon-home config.toml)

When several independent components in one product write the same config file,
last-writer-wins read-modify-write loses updates even when every writer is
low-frequency. This is the protocol that closed that class for daemon-home
`config.toml` (v1.224, issue trail: cross-writer race accepted 2026-10-23, migrated
to the lock protocol once a third writer appeared).

## Protocol

One advisory lock file `<target>.lock` next to the target. Every writer wraps its
**entire** read-modify-write — not just the final write:

1. Acquire: exclusive-create the lock file (`open(path, 'wx', 0o600)` in Node;
   `OpenOptions::create_new(true)` in Rust). On `EEXIST`, retry with backoff
   (25 ms, doubling, 500 ms cap).
2. Stale steal: if the lock file's mtime age exceeds a threshold (10 s here — two
   orders above any live hold for small TOML files), unlink it and retry the
   exclusive create.
3. Hold across read → mutate → write; release by unlinking in a `finally` block
   (TS) or a drop guard (Rust).

## Rules the hard way (v1.224 review findings)

- **Inventory every writer before claiming closure.** The initial analysis said
  "two writers"; the real count was four (TS service profile, two Rust core
  functions, the desktop shell's config mutations, the CLI's config save), plus a
  JSON→TOML migration path that also wrote the file. One unlocked writer keeps the
  lost-update window open and falsifies every interleaving test's premise.
- **The lock window must cover the read too.** A writer that reads an unlocked
  snapshot, then locks only to write, still clobbers a concurrent selection switch
  (found in the CLI logout path — the fix re-validates the freshly-read state
  inside the lock and acts only on a match).
- **One implementation per language, reused everywhere.** The Rust side exports a
  single `with_config_lock` helper consumed by every Rust writer (core + CLI via an
  existing dependency edge); the two TS writers mirror the same constants. A second
  implementation drifts.
- **Typed serialization can drop foreign keys.** A whole-document typed save that
  was correct when it owned the file silently deletes other writers' keys once they
  co-exist. Round-trip the parsed document (flatten-catch for unknown keys) and
  update only owned fields — or justify why the locked read makes the fresh-write
  safe.
- **State the residual honestly.** Exclusive-create locks with mtime stale-steal
  have a real ownership race: a lock that looks stale may belong to a live slow
  holder, and two racers can both hold briefly after a steal. The protocol is safe
  while all writers stay user-paced; it is *not* "unreachable by construction".
  Separately, non-atomic writer bodies (`fs::write`) still expose torn reads to
  unlocked readers — a reader that renames the file on parse failure can move a
  committed document (tracked as its own follow-up).
- **Auxiliary paths count as writers.** A one-time JSON→TOML migration inside the
  loader must take the same lock and re-check the target after acquiring.

## Verification shape

- An interleaved-writer test per implementation side that fails on the unlocked
  code shape (foreign holder blocks, stale lock stolen, no lost key).
- A regression test for every conditional-write path (snapshot A, concurrent
  commit B, act-on-A must preserve B).
- The release path is exception-safe: a failure after exclusive-create success
  must still unlink the file it created, or the next writer waits out the full
  stale threshold for nothing.

## Non-goals / boundaries

- Reader-side locking: reads stay unlocked by design (low frequency, fail-visible).
- Atomic writer bodies (write-temp + rename): a separate improvement; the lock
  protocol does not require it.
- Crash recovery beyond stale-steal: no PID/owner metadata in the lock file — the
  mtime heuristic plus the user-paced writer assumption is the documented boundary.
